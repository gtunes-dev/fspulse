//! Item operations module.
//!
//! The only writer of `items` / `item_versions` for observed filesystem
//! state. Callers describe what is on disk now (`ObservedEvent`); this
//! module decides what, if anything, to write. Not used by the integrity
//! scanner.
//!
//! Single public batch entry point: `apply_observed_events`. It acquires
//! the SQLite write lock and processes the batch under one shared `now`
//! allocated inside the locked section. Callers must NOT pre-allocate
//! timestamps — the lock-allocated-timestamp invariant only holds if
//! `now()` is read while the lock is held.
//!
//! Write rules (see the design doc, sections 2.1–2.4):
//!
//! - An unchanged item writes nothing.
//! - An alive item gets at most one version per hour bucket. A further
//!   change inside the bucket updates that version in place and bumps
//!   `change_count`, unless the version has already been hashed or
//!   validated (a checked version is immutable).
//! - Alive <-> deleted transitions always insert a new version.
//! - A path that is gone tombstones every alive item at that path and,
//!   for folders, every alive descendant.
//! - An item observed with one type tombstones alive items of any other
//!   type at the same path.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::Database;
use crate::error::FsPulseError;
use crate::hierarchy::HierarchyId;
use crate::item_identity::{Access, ItemType, VersionSource};
use crate::snapshots::{self, SnapshotDelta, Totals};
use crate::utils::Utils;
use crate::validate::validator;

/// Width of a coalescing bucket in seconds. An alive item gets at most
/// one version per bucket.
pub const BUCKET_SECS: i64 = 3600;

/// The coalescing bucket a timestamp falls in.
pub fn bucket_of(timestamp: i64) -> i64 {
    timestamp.div_euclid(BUCKET_SECS)
}

// ============================================================================
// Public types
// ============================================================================

/// One filesystem item as it exists on disk right now.
///
/// Owns its strings so callers can build a Vec without holding any
/// borrows. The constructor handles path splitting, type derivation,
/// validator lookup, and folder-specific zeroing of mod_date and size.
/// Fields are `pub(crate)` so callers must use the constructor rather
/// than struct-literal syntax.
#[derive(Debug)]
pub struct ObservedItem {
    pub(crate) parent_path: String,
    pub(crate) item_name: String,
    pub(crate) item_type: ItemType,
    pub(crate) access: Access,
    pub(crate) mod_date: Option<i64>,
    pub(crate) size: Option<i64>,
    pub(crate) file_extension: Option<String>,
    pub(crate) has_validator: bool,
    pub(crate) source: VersionSource,
}

impl ObservedItem {
    /// Build an ObservedItem from a stat result. Caller supplies the
    /// full path, the Metadata it obtained via `symlink_metadata` (or
    /// None if the stat failed), the Access value computed from its
    /// error-handling context, and what produced the observation.
    pub fn new(
        full_path: &Path,
        metadata: Option<&std::fs::Metadata>,
        access: Access,
        source: VersionSource,
    ) -> Result<ObservedItem, FsPulseError> {
        let item_type = match metadata {
            Some(m) if m.is_dir() => ItemType::Directory,
            Some(m) if m.file_type().is_symlink() => ItemType::Symlink,
            Some(m) if m.is_file() => ItemType::File,
            Some(_) => ItemType::Unknown,
            None => ItemType::Unknown,
        };

        let (parent_path, item_name) = split_full_path(full_path)?;

        let is_folder = item_type == ItemType::Directory;
        let mod_date = match metadata {
            Some(m) if !is_folder => m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            _ => None,
        };
        let size = match metadata {
            Some(m) if !is_folder => Some(m.len() as i64),
            _ => None,
        };

        let file_extension = if item_type == ItemType::File {
            validator::file_extension_for_path(full_path)
        } else {
            None
        };
        let has_validator = file_extension
            .as_deref()
            .is_some_and(validator::has_validator_extension);

        Ok(ObservedItem {
            parent_path,
            item_name,
            item_type,
            access,
            mod_date,
            size,
            file_extension,
            has_validator,
            source,
        })
    }

    /// Build a synthetic ObservedItem for a folder we know must exist
    /// but have not actually stat'd. Used only by parent fault-in
    /// inside `resolve_parent_locked`. Folders don't track mod_date or
    /// size, so the only "synthesized" field is access, which is
    /// asserted as Ok. The folder's own observation corrects it if
    /// needed.
    pub(crate) fn synthetic_folder(
        parent_path: String,
        item_name: String,
        source: VersionSource,
    ) -> ObservedItem {
        ObservedItem {
            parent_path,
            item_name,
            item_type: ItemType::Directory,
            access: Access::Ok,
            mod_date: None,
            size: None,
            file_extension: None,
            has_validator: false,
            source,
        }
    }

    /// True if this observation differs from a recorded state in a way
    /// that warrants a write. Folders only change on access transitions;
    /// their mod_date and size are not tracked, to avoid version churn
    /// when contained items change.
    pub(crate) fn differs_from(
        &self,
        access: Access,
        mod_date: Option<i64>,
        size: Option<i64>,
    ) -> bool {
        if self.item_type == ItemType::Directory {
            return access != self.access;
        }
        // A stat cannot tell whether a file's contents are readable, so
        // an observation always reports Ok for one. A recorded ReadError
        // (set by the integrity check) therefore stands until the file's
        // metadata changes; otherwise every walk would flip it back.
        let access_changed =
            access != self.access && !(access == Access::ReadError && self.access == Access::Ok);
        mod_date != self.mod_date || size != self.size || access_changed
    }
}

/// One observation about a path, from the reconcile walk or the watcher.
///
/// Each event carries its own `root_id` so a batch can mix roots and be
/// applied under one lock acquisition with one shared `now`.
#[derive(Debug)]
pub enum ObservedEvent {
    /// An item exists on disk: add, modify, rehydrate, or no-change.
    /// `apply_one_observed_locked` figures out which kind of write to
    /// do based on the item's current state in the database.
    Upsert { root_id: i64, item: ObservedItem },
    /// Nothing exists at this path. Every alive item recorded at the
    /// path is tombstoned, along with all alive descendants of folders.
    Gone {
        root_id: i64,
        path: String,
        source: VersionSource,
    },
}

// ============================================================================
// Public batch entry point
// ============================================================================

/// Why a single event in a batch was skipped instead of applied.
///
/// Skips are per-event data conditions caused by races or stale state
/// upstream of item_ops — for example, an event for a root that was
/// just deleted. They are NOT infrastructure failures: skips do not
/// abort the batch and do not roll back the transaction. Infrastructure
/// failures (rusqlite errors, lock acquisition failures, schema
/// mismatches) propagate as `Err` and abort the whole batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The event's `root_id` does not exist in the `roots` table.
    RootNotFound,
    /// The event's path is not within the root path it claims to
    /// belong to.
    OutsideRoot,
}

/// What applying one event wrote.
#[derive(Debug, Default, Clone, Copy)]
struct Delta {
    added: usize,
    modified: usize,
    coalesced: usize,
    deleted: usize,
    unchanged: usize,
}

/// The result of attempting to apply a single event.
#[derive(Debug, Clone, Copy)]
enum EventOutcome {
    Applied(Delta),
    Skipped(SkipReason),
}

/// Counts produced by a batch apply.
///
/// `added`, `modified` and `deleted` count versions inserted for the
/// event's own item (and, for `deleted`, descendants and items of
/// another type at the same path). `coalesced` counts in-place updates.
/// Ancestors faulted in while resolving a parent are not counted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BatchStats {
    pub applied: usize,
    pub skipped: usize,
    pub added: usize,
    pub modified: usize,
    pub coalesced: usize,
    pub deleted: usize,
    pub unchanged: usize,
}

impl BatchStats {
    /// Fold another batch's counts into this one.
    pub fn absorb(&mut self, other: &BatchStats) {
        self.applied += other.applied;
        self.skipped += other.skipped;
        self.added += other.added;
        self.modified += other.modified;
        self.coalesced += other.coalesced;
        self.deleted += other.deleted;
        self.unchanged += other.unchanged;
    }
}

/// Apply a batch of observed events to the database. Acquires a
/// connection from the pool and takes the SQLite write lock for the
/// duration of the batch. The batch may contain events for multiple
/// roots — each event carries its own `root_id`.
///
/// Per-event data problems (event outside its root, root_id no longer
/// exists) are reported as skips and counted in the returned
/// `BatchStats`. Infrastructure problems (rusqlite errors, lock
/// failures) propagate as `Err` and roll the batch back.
pub fn apply_observed_events(events: &[ObservedEvent]) -> Result<BatchStats, FsPulseError> {
    let conn = Database::get_connection()?;
    Database::immediate_transaction(&conn, |tx| {
        apply_observed_events_at(tx, events, Utils::now_secs())
    })
}

/// Apply a batch with an explicit `now`. The caller must hold the write
/// lock and must have read `now` while holding it. Split out from
/// `apply_observed_events` so tests can drive time.
///
/// Caches `root_id → Option<root_path>` lookups so a cross-root batch
/// only queries `roots` once per distinct root, *and* a known-bad
/// root_id (cache value `None`) is remembered without re-querying for
/// every event that references it.
pub(crate) fn apply_observed_events_at(
    conn: &Connection,
    events: &[ObservedEvent],
    now: i64,
) -> Result<BatchStats, FsPulseError> {
    let mut root_paths: HashMap<i64, Option<String>> = HashMap::new();
    let mut stats = BatchStats::default();
    // What this batch changed in each root, for the root's snapshot.
    let mut snapshot_deltas: HashMap<i64, SnapshotDelta> = HashMap::new();

    for event in events {
        let root_id = match event {
            ObservedEvent::Upsert { root_id, .. } | ObservedEvent::Gone { root_id, .. } => *root_id,
        };

        let outcome = match resolve_root_path_locked(conn, root_id, &mut root_paths)? {
            None => EventOutcome::Skipped(SkipReason::RootNotFound),
            Some(root_path) => {
                let snapshot = snapshot_deltas.entry(root_id).or_default();
                match event {
                    ObservedEvent::Upsert { item, .. } => {
                        apply_one_observed_locked(conn, root_id, root_path, item, now, snapshot)?
                    }
                    ObservedEvent::Gone { path, source, .. } => apply_one_gone_locked(
                        conn, root_id, root_path, path, *source, now, snapshot,
                    )?,
                }
            }
        };

        match outcome {
            EventOutcome::Applied(delta) => {
                stats.applied += 1;
                stats.added += delta.added;
                stats.modified += delta.modified;
                stats.coalesced += delta.coalesced;
                stats.deleted += delta.deleted;
                stats.unchanged += delta.unchanged;
            }
            EventOutcome::Skipped(reason) => {
                stats.skipped += 1;
                log::warn!("item_ops: skipped event ({reason:?}): {event:?}");
            }
        }
    }

    // Same transaction as the writes above, so a snapshot row always
    // matches the versions behind it. A batch that changed nothing
    // writes nothing here either.
    for (root_id, delta) in &snapshot_deltas {
        snapshots::apply_delta_locked(conn, *root_id, delta, now)?;
    }

    Ok(stats)
}

/// Get the cached root_path for `root_id`. Returns `Some(path)` if the
/// root exists, `None` if the `roots` table has no such row (which is
/// remembered in the cache so subsequent events for the same missing
/// root_id don't re-query). Any other database error propagates.
fn resolve_root_path_locked<'a>(
    conn: &Connection,
    root_id: i64,
    cache: &'a mut HashMap<i64, Option<String>>,
) -> Result<Option<&'a str>, FsPulseError> {
    if let std::collections::hash_map::Entry::Vacant(e) = cache.entry(root_id) {
        let path = conn
            .query_row(
                "SELECT root_path FROM roots WHERE root_id = ?",
                params![root_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(FsPulseError::DatabaseError)?;
        e.insert(path);
    }
    Ok(cache.get(&root_id).unwrap().as_deref())
}

// ============================================================================
// apply_one_observed — central per-item write helper
// ============================================================================

/// Apply a single observed item.
///
/// 1. Tombstone alive items of any other type at the same path.
/// 2. Resolve the parent (faulting in missing/tombstoned ancestors).
/// 3. Decide and write, based on the item of the observed type:
///    - No row → create
///    - Tombstoned → insert rehydration version
///    - Alive, changed, current version can absorb it → update in place
///    - Alive, changed → insert modified version
///    - Alive, unchanged → no write
fn apply_one_observed_locked(
    conn: &Connection,
    root_id: i64,
    root_path: &str,
    observed: &ObservedItem,
    now: i64,
    snapshot: &mut SnapshotDelta,
) -> Result<EventOutcome, FsPulseError> {
    // Validate up front, before any writes or fault-in. Skipping after
    // partial fault-in would leave orphaned ancestor writes in the
    // transaction.
    if !path_is_within_root(&observed.parent_path, root_path) {
        return Ok(EventOutcome::Skipped(SkipReason::OutsideRoot));
    }

    let path = join_full_path(&observed.parent_path, &observed.item_name);
    let mut delta = Delta::default();
    let mut current: Option<CurrentItem> = None;

    for item in lookup_items_at_path_locked(conn, root_id, &path)? {
        if item.item_type == observed.item_type {
            current = Some(item);
        } else if !item.is_deleted {
            // The path now holds something of a different type (for
            // example a file replaced by a folder).
            delta.deleted += tombstone_with_descendants_locked(
                conn,
                root_id,
                &item,
                observed.source,
                now,
                snapshot,
            )?;
        }
    }

    let parent = resolve_parent_locked(
        conn,
        root_id,
        root_path,
        &observed.parent_path,
        observed.source,
        now,
        snapshot,
    )?;

    match current {
        None => {
            write_new_item_locked(conn, root_id, parent.as_ref(), observed, &path, now)?;
            delta.added += 1;
            snapshot.record_added(observed.item_type, observed.size);
        }
        Some(state) if state.is_deleted => {
            write_observed_version_locked(conn, root_id, &state, observed, true, now)?;
            delta.added += 1;
            snapshot.record_added(observed.item_type, observed.size);
        }
        Some(state) if observed.differs_from(state.access, state.mod_date, state.size) => {
            let new_version = !state.can_absorb_change(now);
            if new_version {
                write_observed_version_locked(conn, root_id, &state, observed, false, now)?;
                delta.modified += 1;
            } else {
                write_coalesced_change_locked(conn, &state, observed, now)?;
                delta.coalesced += 1;
            }
            snapshot.record_changed(observed.item_type, state.size, observed.size, new_version);
        }
        Some(_) => {
            // Unchanged. Nothing is written: a version stays in effect
            // until the next one begins, and "confirmed as of" is
            // recorded once per root when a reconcile completes.
            delta.unchanged += 1;
        }
    }

    Ok(EventOutcome::Applied(delta))
}

// ============================================================================
// Parent resolution / fault-in
// ============================================================================

/// Information about an alive parent folder needed to insert a child.
struct ResolvedParent {
    item_id: i64,
    hierarchy_id: Vec<u8>,
}

/// Resolve the parent_item_id and parent hierarchy_id for an item
/// whose parent is at `parent_path`. Returns `None` for items that sit
/// directly under the root (no items-row parent).
///
/// Faults in missing or tombstoned ancestors recursively. On the way
/// back down the recursion, each missing ancestor is created and each
/// tombstoned ancestor is rehydrated, top-down. After this function
/// returns Ok, every ancestor of the original child up to the root is
/// guaranteed to be alive in the database.
///
/// Caller is responsible for verifying that `parent_path` is within
/// `root_path` before invoking this function. The check lives at the
/// per-event entry points (`apply_one_observed_locked`) so a failure
/// turns into a clean per-event skip rather than a partial fault-in.
fn resolve_parent_locked(
    conn: &Connection,
    root_id: i64,
    root_path: &str,
    parent_path: &str,
    source: VersionSource,
    now: i64,
    snapshot: &mut SnapshotDelta,
) -> Result<Option<ResolvedParent>, FsPulseError> {
    if Path::new(parent_path) == Path::new(root_path) {
        return Ok(None);
    }

    let existing = lookup_items_at_path_locked(conn, root_id, parent_path)?
        .into_iter()
        .find(|item| item.item_type == ItemType::Directory);

    match existing {
        Some(state) if !state.is_deleted => Ok(Some(ResolvedParent {
            item_id: state.item_id,
            hierarchy_id: state.hierarchy_id,
        })),
        Some(state) => {
            // Tombstoned: rehydrate. Recurse on the grandparent first
            // so we never leave an alive item with a tombstoned
            // ancestor. The grandparent's resolved values aren't
            // needed locally — rehydration reuses the existing items
            // row's parent_item_id and hierarchy_id, which are
            // immutable item-level fields.
            let (gp_path, parent_name) = split_full_path(Path::new(parent_path))?;
            resolve_parent_locked(conn, root_id, root_path, &gp_path, source, now, snapshot)?;
            let synthetic = ObservedItem::synthetic_folder(gp_path, parent_name, source);
            write_observed_version_locked(conn, root_id, &state, &synthetic, true, now)?;
            snapshot.record_added(ItemType::Directory, None);
            Ok(Some(ResolvedParent {
                item_id: state.item_id,
                hierarchy_id: state.hierarchy_id,
            }))
        }
        None => {
            // Missing: resolve grandparent first (recursing as needed),
            // then create a fresh items row + initial version.
            let (gp_path, parent_name) = split_full_path(Path::new(parent_path))?;
            let grandparent =
                resolve_parent_locked(conn, root_id, root_path, &gp_path, source, now, snapshot)?;
            let synthetic = ObservedItem::synthetic_folder(gp_path, parent_name, source);
            let (item_id, hierarchy_id) = write_new_item_locked(
                conn,
                root_id,
                grandparent.as_ref(),
                &synthetic,
                parent_path,
                now,
            )?;
            snapshot.record_added(ItemType::Directory, None);
            Ok(Some(ResolvedParent {
                item_id,
                hierarchy_id,
            }))
        }
    }
}

// ============================================================================
// apply_one_gone — tombstone everything at a path
// ============================================================================

fn apply_one_gone_locked(
    conn: &Connection,
    root_id: i64,
    root_path: &str,
    path: &str,
    source: VersionSource,
    now: i64,
    snapshot: &mut SnapshotDelta,
) -> Result<EventOutcome, FsPulseError> {
    // The root itself is not an item, and a path outside it is not ours.
    let within = Path::new(path)
        .parent()
        .is_some_and(|parent| parent.starts_with(Path::new(root_path)));
    if !within {
        return Ok(EventOutcome::Skipped(SkipReason::OutsideRoot));
    }

    let mut delta = Delta::default();
    for item in lookup_items_at_path_locked(conn, root_id, path)? {
        if !item.is_deleted {
            delta.deleted +=
                tombstone_with_descendants_locked(conn, root_id, &item, source, now, snapshot)?;
        }
    }
    Ok(EventOutcome::Applied(delta))
}

/// Tombstone every alive descendant of a folder.
/// Parameters: ?1 root_id, ?2 the folder's item_id, ?3 now, ?4 source.
///
/// Descendants are found by walking parent_item_id links (index
/// idx_items_parent_name). CROSS JOIN pins the join order to
/// descendants → items → versions; left to itself the planner may scan
/// all of item_versions and probe the descendant set instead.
/// Tombstoning leaves each items row, its parent link and its
/// hierarchy_id intact.
pub(crate) const TOMBSTONE_DESCENDANTS_SQL: &str = "
    WITH RECURSIVE sub(item_id) AS (
        SELECT item_id FROM items
         WHERE root_id = ?1 AND parent_item_id = ?2
        UNION ALL
        SELECT i.item_id FROM sub s
         CROSS JOIN items i ON i.root_id = ?1 AND i.parent_item_id = s.item_id
    )
    INSERT INTO item_versions (
        item_id, item_version, root_id, parent_item_id, hierarchy_id,
        first_seen_at, last_changed_at, change_count, source,
        is_added, is_deleted, access, mod_date, size
    )
    SELECT
        i.item_id, iv.item_version + 1, i.root_id, i.parent_item_id, i.hierarchy_id,
        ?3, ?3, 1, ?4,
        0, 1, iv.access, NULL, NULL
    FROM sub
    CROSS JOIN items i ON i.item_id = sub.item_id
    CROSS JOIN item_versions iv ON iv.item_id = i.item_id
       AND iv.item_version = (
           SELECT MAX(iv2.item_version) FROM item_versions iv2
           WHERE iv2.item_id = i.item_id
       )
    WHERE iv.is_deleted = 0";

/// What is alive under a folder: files, folders, and bytes in files.
/// Parameters: ?1 root_id, ?2 the folder's item_id.
/// The same descendants `TOMBSTONE_DESCENDANTS_SQL` is about to mark
/// deleted, read first so the root's snapshot totals can be reduced by
/// what is removed.
pub(crate) const DESCENDANT_TOTALS_SQL: &str = "
    WITH RECURSIVE sub(item_id) AS (
        SELECT item_id FROM items
         WHERE root_id = ?1 AND parent_item_id = ?2
        UNION ALL
        SELECT i.item_id FROM sub s
         CROSS JOIN items i ON i.root_id = ?1 AND i.parent_item_id = s.item_id
    )
    SELECT COALESCE(SUM(i.item_type = 0), 0),
           COALESCE(SUM(i.item_type = 1), 0),
           COALESCE(SUM(CASE WHEN i.item_type = 0 THEN iv.size END), 0)
    FROM sub
    CROSS JOIN items i ON i.item_id = sub.item_id
    CROSS JOIN item_versions iv ON iv.item_id = i.item_id
       AND iv.item_version = (
           SELECT MAX(iv2.item_version) FROM item_versions iv2
           WHERE iv2.item_id = i.item_id
       )
    WHERE iv.is_deleted = 0";

/// Tombstone an alive item and, if it is a folder, every alive
/// descendant. Returns the number of tombstones written.
fn tombstone_with_descendants_locked(
    conn: &Connection,
    root_id: i64,
    item: &CurrentItem,
    source: VersionSource,
    now: i64,
    snapshot: &mut SnapshotDelta,
) -> Result<usize, FsPulseError> {
    let mut written = 0;

    if item.item_type == ItemType::Directory {
        let totals = conn
            .prepare_cached(DESCENDANT_TOTALS_SQL)?
            .query_row(params![root_id, item.item_id], |row| {
                Ok(Totals {
                    files: row.get(0)?,
                    folders: row.get(1)?,
                    size: row.get(2)?,
                })
            })?;
        written += conn.execute(
            TOMBSTONE_DESCENDANTS_SQL,
            params![root_id, item.item_id, now, source.as_i64()],
        )?;
        snapshot.record_deleted_many(written as i64, totals);
    }

    write_tombstone_version_locked(conn, root_id, item, source, now)?;
    snapshot.record_deleted(item.item_type, item.size);
    Ok(written + 1)
}

// ============================================================================
// Lookup helpers
// ============================================================================

/// Joined snapshot of an items row and its latest item_versions row.
///
/// Item-level columns (item_id, parent_item_id, hierarchy_id) come
/// from `items`, which is the source of truth — the matching columns
/// on `item_versions` are denormalized copies. `hierarchy_id` is
/// non-optional: every items row in v32 must have one, and a NULL
/// here is treated as a corruption error by rusqlite at row decode
/// time.
struct CurrentItem {
    item_id: i64,
    item_type: ItemType,
    item_version: i64,
    parent_item_id: Option<i64>,
    hierarchy_id: Vec<u8>,
    first_seen_at: i64,
    is_deleted: bool,
    access: Access,
    mod_date: Option<i64>,
    size: Option<i64>,
    /// True once the version has been validated or hashed. A checked
    /// version is never updated in place: the hash or validation result
    /// describes exactly the state it recorded.
    is_checked: bool,
}

impl CurrentItem {
    /// True if a change observed at `now` can be folded into this
    /// version instead of starting a new one: the version is alive,
    /// began in the same hour bucket, and has not been hashed or
    /// validated.
    fn can_absorb_change(&self, now: i64) -> bool {
        !self.is_deleted && !self.is_checked && bucket_of(self.first_seen_at) == bucket_of(now)
    }
}

/// Every item recorded at `path` (at most one per item type), each with
/// its latest version.
fn lookup_items_at_path_locked(
    conn: &Connection,
    root_id: i64,
    path: &str,
) -> Result<Vec<CurrentItem>, FsPulseError> {
    let mut stmt = conn.prepare_cached(
        "SELECT i.item_id, i.item_type, i.parent_item_id, i.hierarchy_id,
                iv.item_version, iv.first_seen_at, iv.is_deleted, iv.access,
                iv.mod_date, iv.size,
                (iv.val_state IS NOT NULL OR EXISTS (
                    SELECT 1 FROM hash_versions hv
                     WHERE hv.item_id = iv.item_id AND hv.item_version = iv.item_version
                )) AS is_checked
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
            AND iv.item_version = (
                SELECT MAX(iv2.item_version) FROM item_versions iv2
                WHERE iv2.item_id = i.item_id
            )
         WHERE i.root_id = ?1 AND i.item_path = ?2",
    )?;
    let items = stmt
        .query_map(params![root_id, path], |row| {
            Ok(CurrentItem {
                item_id: row.get(0)?,
                item_type: ItemType::from_i64(row.get(1)?),
                parent_item_id: row.get(2)?,
                hierarchy_id: row.get(3)?,
                item_version: row.get(4)?,
                first_seen_at: row.get(5)?,
                is_deleted: row.get(6)?,
                access: Access::from_i64(row.get(7)?),
                mod_date: row.get(8)?,
                size: row.get(9)?,
                is_checked: row.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(items)
}

// ============================================================================
// Write helpers
// ============================================================================

/// Create a fresh items row plus its initial item_versions row, and
/// return the new item_id and hierarchy_id so callers (specifically
/// fault-in) can chain without re-querying.
fn write_new_item_locked(
    conn: &Connection,
    root_id: i64,
    parent: Option<&ResolvedParent>,
    observed: &ObservedItem,
    path: &str,
    now: i64,
) -> Result<(i64, Vec<u8>), FsPulseError> {
    let parent_item_id = parent.map(|p| p.item_id);
    let parent_hierarchy_id = parent.map(|p| p.hierarchy_id.as_slice());

    let new_hierarchy_id = compute_new_hierarchy_id_locked(
        conn,
        root_id,
        parent_item_id,
        parent_hierarchy_id,
        &observed.item_name,
    )?;

    conn.execute(
        "INSERT INTO items (
            root_id, parent_item_id, item_path, item_name, hierarchy_id,
            item_type, file_extension, has_validator
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            root_id,
            parent_item_id,
            path,
            &observed.item_name,
            &new_hierarchy_id,
            observed.item_type.as_i64(),
            observed.file_extension.as_deref(),
            observed.has_validator as i64,
        ],
    )?;
    let item_id = conn.last_insert_rowid();

    insert_item_version_locked(
        conn,
        item_id,
        1,
        root_id,
        parent_item_id,
        &new_hierarchy_id,
        now,
        observed.source,
        true,
        false,
        observed.access,
        observed.mod_date,
        observed.size,
    )?;

    Ok((item_id, new_hierarchy_id))
}

/// Fold a change into the item's current version: overwrite its state,
/// stamp `last_changed_at`, and count the change. Only valid when
/// `CurrentItem::can_absorb_change` holds.
fn write_coalesced_change_locked(
    conn: &Connection,
    current: &CurrentItem,
    observed: &ObservedItem,
    now: i64,
) -> Result<(), FsPulseError> {
    conn.execute(
        "UPDATE item_versions
            SET access = ?1, mod_date = ?2, size = ?3,
                last_changed_at = ?4, change_count = change_count + 1
          WHERE item_id = ?5 AND item_version = ?6",
        params![
            observed.access.as_i64(),
            observed.mod_date,
            observed.size,
            now,
            current.item_id,
            current.item_version,
        ],
    )?;
    Ok(())
}

/// Insert a new item_versions row reflecting an observation against an
/// existing items row. `is_added=true` is the rehydration case
/// (previous version was tombstoned); `is_added=false` is the modified
/// case (previous version was alive but metadata differs). Reuses the
/// items row's immutable parent_item_id and hierarchy_id.
fn write_observed_version_locked(
    conn: &Connection,
    root_id: i64,
    current: &CurrentItem,
    observed: &ObservedItem,
    is_added: bool,
    now: i64,
) -> Result<(), FsPulseError> {
    insert_item_version_locked(
        conn,
        current.item_id,
        current.item_version + 1,
        root_id,
        current.parent_item_id,
        &current.hierarchy_id,
        now,
        observed.source,
        is_added,
        false,
        observed.access,
        observed.mod_date,
        observed.size,
    )
}

/// Insert a tombstone item_versions row. Reuses the alive version's
/// access value (the last known state for this path) and writes NULL
/// for mod_date and size since neither is meaningful for a deleted
/// item.
fn write_tombstone_version_locked(
    conn: &Connection,
    root_id: i64,
    current: &CurrentItem,
    source: VersionSource,
    now: i64,
) -> Result<(), FsPulseError> {
    insert_item_version_locked(
        conn,
        current.item_id,
        current.item_version + 1,
        root_id,
        current.parent_item_id,
        &current.hierarchy_id,
        now,
        source,
        false,
        true,
        current.access,
        None,
        None,
    )
}

/// The single per-row item_versions INSERT. All new versions stamp
/// `first_seen_at = last_changed_at = now` with `change_count = 1`.
#[allow(clippy::too_many_arguments)]
fn insert_item_version_locked(
    conn: &Connection,
    item_id: i64,
    item_version: i64,
    root_id: i64,
    parent_item_id: Option<i64>,
    hierarchy_id: &[u8],
    now: i64,
    source: VersionSource,
    is_added: bool,
    is_deleted: bool,
    access: Access,
    mod_date: Option<i64>,
    size: Option<i64>,
) -> Result<(), FsPulseError> {
    conn.execute(
        "INSERT INTO item_versions (
            item_id, item_version, root_id, parent_item_id, hierarchy_id,
            first_seen_at, last_changed_at, change_count, source,
            is_added, is_deleted, access, mod_date, size
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 1, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            item_id,
            item_version,
            root_id,
            parent_item_id,
            hierarchy_id,
            now,
            source.as_i64(),
            is_added as i64,
            is_deleted as i64,
            access.as_i64(),
            mod_date,
            size,
        ],
    )?;
    Ok(())
}

// ============================================================================
// Hierarchy id assignment
// ============================================================================

/// The two siblings a new child named ?3 sorts between, under parent ?2
/// in root ?1. Returns exactly two rows (left, then right); column 1 is
/// the neighbor's hierarchy_id, NULL when there is none on that side.
const NEIGHBORS_SQL: &str = "
        SELECT 0 AS ord, (
            -- column 1: the neighbor's hierarchy_id (NULL if none)
            SELECT hierarchy_id FROM items
             WHERE root_id = ?1 AND parent_item_id IS ?2
               AND item_name COLLATE natural_path <= ?3
             ORDER BY item_name COLLATE natural_path DESC, hierarchy_id DESC
             LIMIT 1
        )
        UNION ALL
        SELECT 1, (
            SELECT hierarchy_id FROM items
             WHERE root_id = ?1 AND parent_item_id IS ?2
               AND item_name COLLATE natural_path > ?3
             ORDER BY item_name COLLATE natural_path ASC, hierarchy_id ASC
             LIMIT 1
        )
        ORDER BY ord";

/// Compute a new hierarchy_id for a child being inserted under
/// `parent_item_id`, preserving the natural_path sort order of `name`
/// among existing siblings.
///
/// One round-trip fetches both neighbors. The query returns exactly
/// two rows, ordered left-then-right by the synthetic `ord` column.
/// Each branch's `hierarchy_id` is NULL when no neighbor exists on
/// that side. Sibling lookups query `items` only and do NOT filter by
/// aliveness — hierarchy_id ordering must be consistent regardless of
/// liveness because UI paginators show alive and deleted items in the
/// same sorted view, and a tombstoned sibling still occupies its
/// hierarchy_id slot.
///
/// The left neighbor is the last sibling whose name sorts at or before
/// `name`. "At" matters: a path can exist as items of different types
/// (a file later replaced by a folder), and the new item must be placed
/// after its same-named sibling rather than computed from the same two
/// neighbors, which would hand out the same hierarchy_id twice.
///
/// `parent_item_id IS ?` matches both NULL and integer values when
/// bound from `Option<i64>`, so the same SQL handles root-level and
/// nested items.
fn compute_new_hierarchy_id_locked(
    conn: &Connection,
    root_id: i64,
    parent_item_id: Option<i64>,
    parent_hierarchy_id: Option<&[u8]>,
    name: &str,
) -> Result<Vec<u8>, FsPulseError> {
    let mut stmt = conn.prepare_cached(NEIGHBORS_SQL)?;
    let mut rows = stmt.query(params![root_id, parent_item_id, name])?;
    let left_bytes: Option<Vec<u8>> = rows
        .next()?
        .ok_or_else(|| FsPulseError::Error("neighbor query missing left row".into()))?
        .get(1)?;
    let right_bytes: Option<Vec<u8>> = rows
        .next()?
        .ok_or_else(|| FsPulseError::Error("neighbor query missing right row".into()))?
        .get(1)?;

    let left = left_bytes.as_deref().map(HierarchyId::from_bytes);
    let right = right_bytes.as_deref().map(HierarchyId::from_bytes);

    let parent = match parent_hierarchy_id {
        Some(bytes) => HierarchyId::from_bytes(bytes),
        None => HierarchyId::get_root(),
    };

    Ok(parent.get_descendant(left.as_ref(), right.as_ref()).to_vec())
}

// ============================================================================
// Small utilities
// ============================================================================

/// Split a full path into its parent path and file name components.
/// Errors on malformed input (no parent or no file_name) rather than
/// silently producing empty strings.
fn split_full_path(full_path: &Path) -> Result<(String, String), FsPulseError> {
    let parent_path = full_path
        .parent()
        .ok_or_else(|| {
            FsPulseError::Error(format!("path '{}' has no parent", full_path.display()))
        })?
        .to_string_lossy()
        .into_owned();
    let item_name = full_path
        .file_name()
        .ok_or_else(|| {
            FsPulseError::Error(format!("path '{}' has no file_name", full_path.display()))
        })?
        .to_string_lossy()
        .into_owned();
    Ok((parent_path, item_name))
}

/// Reconstruct a full path from a parent path and file name, using
/// platform-native separators (matches how `items.item_path` is stored).
fn join_full_path(parent: &str, name: &str) -> String {
    Path::new(parent).join(name).to_string_lossy().into_owned()
}

fn path_is_within_root(parent_path: &str, root_path: &str) -> bool {
    Path::new(parent_path).starts_with(Path::new(root_path))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::db::schema::CREATE_SCHEMA_SQL;
    use crate::sort::compare_paths;

    pub const ROOT: &str = "/r";

    /// In-memory database with the current schema and one root at `/r`.
    /// Returns the connection and the root_id.
    pub fn test_db() -> (Connection, i64) {
        test_db_with_root(ROOT)
    }

    pub fn test_db_with_root(root_path: &str) -> (Connection, i64) {
        let conn = Connection::open_in_memory().unwrap();
        conn.create_collation("natural_path", compare_paths).unwrap();
        conn.execute_batch(CREATE_SCHEMA_SQL).unwrap();
        conn.execute("INSERT INTO roots (root_path) VALUES (?1)", [root_path])
            .unwrap();
        let root_id = conn.last_insert_rowid();
        (conn, root_id)
    }

    pub fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |row| row.get(0)).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    /// A clock time comfortably inside an hour bucket.
    const T0: i64 = 1_700_000_000 - (1_700_000_000 % BUCKET_SECS) + 600;

    fn file(path: &str, mod_date: i64, size: i64) -> ObservedItem {
        let (parent_path, item_name) = split_full_path(Path::new(path)).unwrap();
        ObservedItem {
            parent_path,
            item_name,
            item_type: ItemType::File,
            access: Access::Ok,
            mod_date: Some(mod_date),
            size: Some(size),
            file_extension: None,
            has_validator: false,
            source: VersionSource::Watch,
        }
    }

    fn folder(path: &str) -> ObservedItem {
        let (parent_path, item_name) = split_full_path(Path::new(path)).unwrap();
        ObservedItem::synthetic_folder(parent_path, item_name, VersionSource::Watch)
    }

    fn upsert(conn: &Connection, root_id: i64, item: ObservedItem, now: i64) -> BatchStats {
        apply_observed_events_at(conn, &[ObservedEvent::Upsert { root_id, item }], now).unwrap()
    }

    fn gone(conn: &Connection, root_id: i64, path: &str, now: i64) -> BatchStats {
        let event = ObservedEvent::Gone {
            root_id,
            path: path.to_string(),
            source: VersionSource::Watch,
        };
        apply_observed_events_at(conn, &[event], now).unwrap()
    }

    /// (version number, change_count, size, is_deleted) of an item's
    /// latest version.
    fn latest(conn: &Connection, path: &str, item_type: ItemType) -> (i64, i64, Option<i64>, bool) {
        conn.query_row(
            "SELECT iv.item_version, iv.change_count, iv.size, iv.is_deleted
             FROM items i JOIN item_versions iv ON iv.item_id = i.item_id
             WHERE i.item_path = ?1 AND i.item_type = ?2
             ORDER BY iv.item_version DESC LIMIT 1",
            params![path, item_type.as_i64()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap()
    }

    fn hierarchy_id(conn: &Connection, path: &str, item_type: ItemType) -> HierarchyId {
        let bytes: Vec<u8> = conn
            .query_row(
                "SELECT hierarchy_id FROM items WHERE item_path = ?1 AND item_type = ?2",
                params![path, item_type.as_i64()],
                |row| row.get(0),
            )
            .unwrap();
        HierarchyId::from_bytes(&bytes)
    }

    #[test]
    fn new_file_faults_in_missing_ancestors() {
        let (conn, root_id) = test_db();
        let stats = upsert(&conn, root_id, file("/r/a/b/f.txt", 10, 100), T0);

        assert_eq!(stats.added, 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM items"), 3);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM item_versions WHERE is_added = 1"), 3);

        let a = hierarchy_id(&conn, "/r/a", ItemType::Directory);
        let b = hierarchy_id(&conn, "/r/a/b", ItemType::Directory);
        let f = hierarchy_id(&conn, "/r/a/b/f.txt", ItemType::File);
        assert!(b.is_descendant_of(&a));
        assert!(f.is_descendant_of(&b));
    }

    #[test]
    fn unchanged_item_writes_nothing() {
        let (conn, root_id) = test_db();
        upsert(&conn, root_id, file("/r/f.txt", 10, 100), T0);

        let before = conn.total_changes();
        let stats = upsert(&conn, root_id, file("/r/f.txt", 10, 100), T0 + 5 * BUCKET_SECS);

        assert_eq!(stats.unchanged, 1);
        assert_eq!(conn.total_changes(), before);
    }

    #[test]
    fn changes_within_the_hour_coalesce_into_one_version() {
        let (conn, root_id) = test_db();
        upsert(&conn, root_id, file("/r/f.txt", 10, 100), T0);
        let s1 = upsert(&conn, root_id, file("/r/f.txt", 20, 200), T0 + 60);
        let s2 = upsert(&conn, root_id, file("/r/f.txt", 30, 300), T0 + 120);

        assert_eq!((s1.coalesced, s2.coalesced), (1, 1));
        assert_eq!(latest(&conn, "/r/f.txt", ItemType::File), (1, 3, Some(300), false));
        let (first_seen, last_changed): (i64, i64) = conn
            .query_row(
                "SELECT first_seen_at, last_changed_at FROM item_versions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((first_seen, last_changed), (T0, T0 + 120));
    }

    #[test]
    fn change_in_a_later_hour_starts_a_new_version() {
        let (conn, root_id) = test_db();
        upsert(&conn, root_id, file("/r/f.txt", 10, 100), T0);
        let stats = upsert(&conn, root_id, file("/r/f.txt", 20, 200), T0 + BUCKET_SECS);

        assert_eq!(stats.modified, 1);
        assert_eq!(latest(&conn, "/r/f.txt", ItemType::File), (2, 1, Some(200), false));
        assert_eq!(
            count(&conn, "SELECT size FROM item_versions WHERE item_version = 1"),
            100,
            "the earlier version must be left as it was"
        );
    }

    #[test]
    fn a_hashed_version_is_never_updated_in_place() {
        let (conn, root_id) = test_db();
        upsert(&conn, root_id, file("/r/f.txt", 10, 100), T0);
        conn.execute(
            "INSERT INTO hash_versions (item_id, item_version, first_seen_at, last_seen_at, file_hash, hash_state)
             SELECT item_id, 1, ?1, ?1, x'00', 1 FROM items",
            [T0 + 10],
        )
        .unwrap();

        let stats = upsert(&conn, root_id, file("/r/f.txt", 20, 200), T0 + 60);

        assert_eq!((stats.modified, stats.coalesced), (1, 0));
        assert_eq!(latest(&conn, "/r/f.txt", ItemType::File), (2, 1, Some(200), false));
    }

    #[test]
    fn a_validated_version_is_never_updated_in_place() {
        let (conn, root_id) = test_db();
        upsert(&conn, root_id, file("/r/f.txt", 10, 100), T0);
        conn.execute("UPDATE item_versions SET val_state = 1, val_at = ?1", [T0 + 10])
            .unwrap();

        let stats = upsert(&conn, root_id, file("/r/f.txt", 20, 200), T0 + 60);

        assert_eq!((stats.modified, stats.coalesced), (1, 0));
    }

    #[test]
    fn delete_and_recreate_always_make_new_versions() {
        let (conn, root_id) = test_db();
        upsert(&conn, root_id, file("/r/f.txt", 10, 100), T0);
        let deleted = gone(&conn, root_id, "/r/f.txt", T0 + 60);
        let recreated = upsert(&conn, root_id, file("/r/f.txt", 20, 200), T0 + 120);

        assert_eq!(deleted.deleted, 1);
        assert_eq!(recreated.added, 1);
        assert_eq!(latest(&conn, "/r/f.txt", ItemType::File), (3, 1, Some(200), false));
        assert_eq!(
            count(&conn, "SELECT is_added FROM item_versions WHERE item_version = 3"),
            1
        );

        // A change after the re-creation, still in the same hour, folds
        // into the re-created version.
        let changed = upsert(&conn, root_id, file("/r/f.txt", 30, 300), T0 + 180);
        assert_eq!(changed.coalesced, 1);
        assert_eq!(latest(&conn, "/r/f.txt", ItemType::File), (3, 2, Some(300), false));
    }

    #[test]
    fn gone_folder_tombstones_its_whole_subtree() {
        let (conn, root_id) = test_db();
        upsert(&conn, root_id, file("/r/a/b/one.txt", 10, 1), T0);
        upsert(&conn, root_id, file("/r/a/b/two.txt", 10, 2), T0);
        upsert(&conn, root_id, file("/r/a/three.txt", 10, 3), T0);
        upsert(&conn, root_id, file("/r/keep.txt", 10, 4), T0);

        let stats = gone(&conn, root_id, "/r/a", T0 + 60);

        // a, a/b, one, two, three
        assert_eq!(stats.deleted, 5);
        assert!(latest(&conn, "/r/a/b/one.txt", ItemType::File).3);
        assert!(latest(&conn, "/r/a/b", ItemType::Directory).3);
        assert!(!latest(&conn, "/r/keep.txt", ItemType::File).3);

        // Reporting the same path gone again is a no-op.
        let before = conn.total_changes();
        assert_eq!(gone(&conn, root_id, "/r/a", T0 + 120).deleted, 0);
        assert_eq!(conn.total_changes(), before);
    }

    #[test]
    fn gone_path_that_was_never_recorded_writes_nothing() {
        let (conn, root_id) = test_db();
        let before = conn.total_changes();
        let stats = gone(&conn, root_id, "/r/tmp-123", T0);
        assert_eq!((stats.applied, stats.deleted), (1, 0));
        assert_eq!(conn.total_changes(), before);
    }

    #[test]
    fn file_replaced_by_folder_tombstones_the_file() {
        let (conn, root_id) = test_db();
        upsert(&conn, root_id, file("/r/a", 10, 1), T0);
        upsert(&conn, root_id, file("/r/b", 10, 1), T0);
        upsert(&conn, root_id, file("/r/c", 10, 1), T0);

        let stats = upsert(&conn, root_id, folder("/r/b"), T0 + 60);

        assert_eq!((stats.added, stats.deleted), (1, 1));
        assert!(latest(&conn, "/r/b", ItemType::File).3);
        assert!(!latest(&conn, "/r/b", ItemType::Directory).3);

        // Same-named siblings must not share a hierarchy_id, and name
        // order must be preserved around them.
        assert_eq!(
            count(&conn, "SELECT COUNT(DISTINCT hierarchy_id) FROM items"),
            4
        );
        let a = hierarchy_id(&conn, "/r/a", ItemType::File);
        let b_file = hierarchy_id(&conn, "/r/b", ItemType::File);
        let b_dir = hierarchy_id(&conn, "/r/b", ItemType::Directory);
        let c = hierarchy_id(&conn, "/r/c", ItemType::File);
        assert!(a < b_file && b_file < b_dir && b_dir < c);
    }

    #[test]
    fn folder_replaced_by_file_tombstones_the_folder_and_its_contents() {
        let (conn, root_id) = test_db();
        upsert(&conn, root_id, file("/r/x/inner.txt", 10, 1), T0);

        let stats = upsert(&conn, root_id, file("/r/x", 20, 2), T0 + 60);

        assert_eq!((stats.added, stats.deleted), (1, 2));
        assert!(latest(&conn, "/r/x/inner.txt", ItemType::File).3);
    }

    #[test]
    fn new_child_rehydrates_a_tombstoned_parent() {
        let (conn, root_id) = test_db();
        upsert(&conn, root_id, file("/r/a/one.txt", 10, 1), T0);
        gone(&conn, root_id, "/r/a", T0 + 60);

        upsert(&conn, root_id, file("/r/a/two.txt", 10, 2), T0 + 120);

        assert_eq!(latest(&conn, "/r/a", ItemType::Directory), (3, 1, None, false));
        assert!(latest(&conn, "/r/a/one.txt", ItemType::File).3);
        assert!(!latest(&conn, "/r/a/two.txt", ItemType::File).3);
    }

    #[test]
    fn events_outside_the_root_or_for_unknown_roots_are_skipped() {
        let (conn, root_id) = test_db();
        let before = conn.total_changes();

        let outside = upsert(&conn, root_id, file("/elsewhere/f.txt", 10, 1), T0);
        let the_root = gone(&conn, root_id, "/r", T0);
        let no_root = upsert(&conn, root_id + 99, file("/r/f.txt", 10, 1), T0);

        assert_eq!((outside.skipped, the_root.skipped, no_root.skipped), (1, 1, 1));
        assert_eq!(conn.total_changes(), before);
    }

    #[test]
    fn siblings_keep_natural_name_order() {
        let (conn, root_id) = test_db();
        for name in ["file10", "file2", "file1", "alpha", "zeta"] {
            upsert(&conn, root_id, file(&format!("/r/{name}"), 10, 1), T0);
        }
        let ordered: Vec<String> = conn
            .prepare("SELECT item_name FROM items ORDER BY hierarchy_id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(ordered, ["alpha", "file1", "file2", "file10", "zeta"]);
    }

    /// Prints and checks the query plans of the hot reconcile queries
    /// against a real database. Ignored by default:
    ///
    ///   FSPULSE_PLAN_TEST_DB=.local/fspulse.db \
    ///     cargo test hot_queries_use_the_parent_index -- --ignored --nocapture
    ///
    /// The database is opened read-only. Plans must be checked through
    /// this connection, not the sqlite3 CLI: the CLI has no
    /// `natural_path` collation and cannot use idx_items_parent_name.
    #[test]
    #[ignore]
    fn hot_queries_use_the_parent_index() {
        use crate::sort::compare_paths;
        use rusqlite::OpenFlags;

        let path = std::env::var("FSPULSE_PLAN_TEST_DB").expect("set FSPULSE_PLAN_TEST_DB");
        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        conn.create_collation("natural_path", compare_paths).unwrap();

        let plan = |label: &str, sql: &str, binds: &[&dyn rusqlite::ToSql]| -> String {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let lines: Vec<String> = stmt
                .query_map(binds, |row| row.get::<_, String>(3))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            let text = lines.join("\n  ");
            eprintln!("{label}:\n  {text}\n");
            text
        };

        let children = plan(
            "children of a directory",
            crate::reconcile::RECORDED_CHILDREN_SQL,
            &[&1i64, &Some(1i64)],
        );
        assert!(children.contains("idx_items_parent_name"));
        assert!(!children.contains("SCAN"));

        let tombstone = plan(
            "tombstone descendants",
            TOMBSTONE_DESCENDANTS_SQL,
            &[&1i64, &1i64, &0i64, &1i64],
        );
        assert!(tombstone.contains("idx_items_parent_name"));
        assert!(!tombstone.contains("SCAN iv"));
        assert!(!tombstone.contains("SCAN i "));

        let alive_descendants = plan(
            "alive descendants",
            crate::reconcile::ALIVE_DESCENDANTS_SQL,
            &[&1i64, &1i64],
        );
        assert!(alive_descendants.contains("idx_items_parent_name"));
        assert!(!alive_descendants.contains("SCAN iv"));
        assert!(!alive_descendants.contains("SCAN i "));

        let descendant_totals = plan(
            "descendant totals",
            DESCENDANT_TOTALS_SQL,
            &[&1i64, &1i64],
        );
        assert!(descendant_totals.contains("idx_items_parent_name"));
        assert!(!descendant_totals.contains("SCAN iv"));
        assert!(!descendant_totals.contains("SCAN i "));

        // The whole-root count reads every item of the root by design.
        // Print its plan and how long it takes on the largest root.
        plan("alive totals", crate::snapshots::ALIVE_TOTALS_SQL, &[&1i64]);
        let largest_root: i64 = conn
            .query_row(
                "SELECT root_id FROM items GROUP BY root_id ORDER BY COUNT(*) DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let started = std::time::Instant::now();
        let totals: (i64, i64, i64) = conn
            .query_row(crate::snapshots::ALIVE_TOTALS_SQL, [largest_root], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
        eprintln!(
            "alive totals of root {largest_root}: {totals:?} in {:?}\n",
            started.elapsed()
        );

        let neighbors = plan(
            "sibling neighbors",
            NEIGHBORS_SQL,
            &[&1i64, &Some(1i64), &"name"],
        );
        assert!(neighbors.contains("idx_items_parent_name"));
    }
}
