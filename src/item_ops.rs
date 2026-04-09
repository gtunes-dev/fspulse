//! Item operations module.
//!
//! Provides the shared write helpers used by both checkpoint and the
//! watcher to apply observed filesystem state to the database. Not used
//! by the integrity scanner.
//!
//! Single public batch entry point: `apply_observed_events`. Callers
//! collect a batch of events (no lock held), then call
//! `apply_observed_events`, which acquires the SQLite write lock and
//! processes the batch under one shared `now` allocated inside the
//! locked section. Callers must NOT pre-allocate timestamps — the
//! lock-allocated-timestamp invariant only holds if `now()` is read
//! while the lock is held.
//!
//! There is no `ItemOp` enum, no `OpResult::Skipped`, no assess/execute
//! split, and no optimistic `WHERE EXISTS` guards. The lock makes all
//! of that unnecessary.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::Database;
use crate::error::FsPulseError;
use crate::hierarchy::HierarchyId;
use crate::item_identity::{Access, ItemType};
use crate::utils::Utils;
use crate::validate::validator;

// ============================================================================
// Public types
// ============================================================================

/// One filesystem item observed by checkpoint or by the watcher.
///
/// Owns its strings so callers can build a Vec without holding any
/// borrows. Constructors handle path splitting, type derivation,
/// validator lookup, and folder-specific zeroing of mod_date and size.
/// Fields are `pub(crate)` so callers must use the constructors rather
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
}

impl ObservedItem {
    /// Build an ObservedItem from a checkpoint walk entry. Caller
    /// supplies the full path, the Metadata it obtained via
    /// symlink_metadata (or None if the stat failed), and the Access
    /// value computed from its error-handling context.
    pub fn from_checkpoint(
        full_path: &Path,
        metadata: Option<&std::fs::Metadata>,
        access: Access,
    ) -> Result<ObservedItem, FsPulseError> {
        Self::build(full_path, metadata, access)
    }

    /// Build an ObservedItem from a watcher event. The watcher must
    /// stat the file in response to the notify event and pass the
    /// resulting Metadata in.
    pub fn from_watcher(
        full_path: &Path,
        metadata: &std::fs::Metadata,
    ) -> Result<ObservedItem, FsPulseError> {
        Self::build(full_path, Some(metadata), Access::Ok)
    }

    /// Build a synthetic ObservedItem for a folder we know must exist
    /// but have not actually stat'd. Used only by parent fault-in
    /// inside `resolve_parent_locked`. Folders in v32 don't track
    /// mod_date or size, so the only "synthesized" field is access,
    /// which is asserted as Ok. The next checkpoint walk over the
    /// affected subtree will refresh access if needed.
    pub(crate) fn synthetic_folder(parent_path: String, item_name: String) -> ObservedItem {
        ObservedItem {
            parent_path,
            item_name,
            item_type: ItemType::Directory,
            access: Access::Ok,
            mod_date: None,
            size: None,
            file_extension: None,
            has_validator: false,
        }
    }

    fn build(
        full_path: &Path,
        metadata: Option<&std::fs::Metadata>,
        access: Access,
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
        })
    }
}

/// One delete event from the watcher. (Checkpoint never produces
/// these — checkpoint deletions are handled by the sweep phase.)
#[derive(Debug)]
pub struct DeletedItem {
    pub(crate) parent_path: String,
    pub(crate) item_name: String,
    pub(crate) item_type: ItemType,
}

impl DeletedItem {
    /// Build a DeletedItem from a watcher delete event. The watcher
    /// passes the full path of the affected item and its type (which
    /// notify provides for delete events).
    pub fn from_watcher_event(
        full_path: &Path,
        item_type: ItemType,
    ) -> Result<DeletedItem, FsPulseError> {
        let (parent_path, item_name) = split_full_path(full_path)?;
        Ok(DeletedItem {
            parent_path,
            item_name,
            item_type,
        })
    }
}

/// One observed event from either checkpoint or the watcher.
///
/// Each event carries its own `root_id` because the watcher reads from
/// a single mpsc channel that intermixes events from every watched
/// root. Tagging at the source lets `apply_observed_events` process a
/// cross-root batch under one lock acquisition with one shared `now`.
#[derive(Debug)]
pub enum ObservedEvent {
    /// An item observed on disk: add, modify, rehydrate, or no-change.
    /// `apply_one_observed_locked` figures out which kind of write to
    /// do based on the item's current state in the database.
    Upsert { root_id: i64, item: ObservedItem },
    /// An item observed to be gone (watcher delete event only).
    Delete { root_id: i64, item: DeletedItem },
}

// ============================================================================
// Public batch entry point
// ============================================================================

/// Why a single event in a batch was skipped instead of applied.
///
/// Skips are per-event data conditions caused by races or stale state
/// upstream of item_ops — for example, the watcher delivering an event
/// for a root that was just deleted, or for a path that no longer
/// belongs to any watched root after a config change. They are NOT
/// infrastructure failures: skips do not abort the batch and do not
/// roll back the transaction. Infrastructure failures (rusqlite
/// errors, lock acquisition failures, schema mismatches) propagate as
/// `Err` and abort the whole batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The event's `root_id` does not exist in the `roots` table. The
    /// watcher most likely captured this event before the root was
    /// removed and is delivering it after.
    RootNotFound,
    /// The event's path is not within the root path it claims to
    /// belong to. Indicates the watcher's prefix-match-to-root logic
    /// produced a mis-tag, or the root_path changed between event
    /// capture and apply.
    OutsideRoot,
}

/// The result of attempting to apply a single event.
#[derive(Debug, Clone, Copy)]
enum EventOutcome {
    Applied,
    Skipped(SkipReason),
}

/// Counts produced by a batch apply. Returned to callers so the
/// watcher / checkpoint can surface skip rates.
#[derive(Debug, Default, Clone, Copy)]
pub struct BatchStats {
    pub applied: usize,
    pub skipped: usize,
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
    Database::immediate_transaction(&conn, |tx| apply_observed_events_locked(tx, events))
}

/// Locked body of `apply_observed_events`.
///
/// Allocates one `now` for the entire batch. The lock guarantees this
/// `now` is `>=` every previously-locked `now`, and that every
/// future-locked `now` will be `>=` this one. We do NOT assume strict
/// inequality in either direction.
///
/// Caches `root_id → Option<root_path>` lookups so a cross-root batch
/// only queries `roots` once per distinct root, *and* a known-bad
/// root_id (cache value `None`) is remembered without re-querying for
/// every event that references it.
fn apply_observed_events_locked(
    conn: &Connection,
    events: &[ObservedEvent],
) -> Result<BatchStats, FsPulseError> {
    let now = Utils::now_secs();
    let mut root_paths: HashMap<i64, Option<String>> = HashMap::new();
    let mut stats = BatchStats::default();

    for event in events {
        let outcome = match event {
            ObservedEvent::Upsert { root_id, item } => {
                match resolve_root_path_locked(conn, *root_id, &mut root_paths)? {
                    Some(root_path) => {
                        apply_one_observed_locked(conn, *root_id, root_path, item, now)?
                    }
                    None => EventOutcome::Skipped(SkipReason::RootNotFound),
                }
            }
            ObservedEvent::Delete { root_id, item } => {
                match resolve_root_path_locked(conn, *root_id, &mut root_paths)? {
                    Some(root_path) => {
                        apply_one_deleted_locked(conn, *root_id, root_path, item, now)?
                    }
                    None => EventOutcome::Skipped(SkipReason::RootNotFound),
                }
            }
        };

        match outcome {
            EventOutcome::Applied => stats.applied += 1,
            EventOutcome::Skipped(reason) => {
                stats.skipped += 1;
                log::warn!("item_ops: skipped event ({reason:?}): {event:?}");
            }
        }
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
    if !cache.contains_key(&root_id) {
        let path = conn
            .query_row(
                "SELECT root_path FROM roots WHERE root_id = ?",
                params![root_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(FsPulseError::DatabaseError)?;
        cache.insert(root_id, path);
    }
    Ok(cache.get(&root_id).unwrap().as_deref())
}

// ============================================================================
// apply_one_observed — central per-item write helper
// ============================================================================

/// Apply a single observed item.
///
/// 1. Resolve the parent (faulting in missing/tombstoned ancestors).
/// 2. Look up the item by (root_id, path, item_type).
/// 3. Decide and write:
///    - No row → create
///    - Tombstoned → insert rehydration version
///    - Alive, changed → insert modified version
///    - Alive, unchanged → extend last_seen_at
fn apply_one_observed_locked(
    conn: &Connection,
    root_id: i64,
    root_path: &str,
    observed: &ObservedItem,
    now: i64,
) -> Result<EventOutcome, FsPulseError> {
    // Validate up front, before any writes or fault-in. Skipping after
    // partial fault-in would leave orphaned ancestor writes in the
    // transaction.
    if !path_is_within_root(&observed.parent_path, root_path) {
        return Ok(EventOutcome::Skipped(SkipReason::OutsideRoot));
    }

    let parent = resolve_parent_locked(conn, root_id, root_path, &observed.parent_path, now)?;
    let path = join_full_path(&observed.parent_path, &observed.item_name);
    let current = lookup_current_item_locked(conn, root_id, &path, observed.item_type)?;

    match current {
        None => {
            write_new_item_locked(conn, root_id, parent.as_ref(), observed, &path, now)?;
        }
        Some(state) if state.is_deleted => {
            write_observed_version_locked(conn, root_id, &state, observed, true, now)?;
        }
        Some(state) if metadata_changed(&state, observed) => {
            write_observed_version_locked(conn, root_id, &state, observed, false, now)?;
        }
        Some(state) => {
            write_extend_last_seen_locked(conn, state.item_id, state.item_version, now)?;
        }
    }

    Ok(EventOutcome::Applied)
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
    now: i64,
) -> Result<Option<ResolvedParent>, FsPulseError> {
    if parent_path == root_path {
        return Ok(None);
    }

    let existing = lookup_current_item_locked(conn, root_id, parent_path, ItemType::Directory)?;

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
            resolve_parent_locked(conn, root_id, root_path, &gp_path, now)?;
            let synthetic = ObservedItem::synthetic_folder(gp_path, parent_name);
            write_observed_version_locked(conn, root_id, &state, &synthetic, true, now)?;
            Ok(Some(ResolvedParent {
                item_id: state.item_id,
                hierarchy_id: state.hierarchy_id,
            }))
        }
        None => {
            // Missing: resolve grandparent first (recursing as needed),
            // then create a fresh items row + initial version.
            let (gp_path, parent_name) = split_full_path(Path::new(parent_path))?;
            let grandparent = resolve_parent_locked(conn, root_id, root_path, &gp_path, now)?;
            let synthetic = ObservedItem::synthetic_folder(gp_path, parent_name);
            let (item_id, hierarchy_id) = write_new_item_locked(
                conn,
                root_id,
                grandparent.as_ref(),
                &synthetic,
                parent_path,
                now,
            )?;
            Ok(Some(ResolvedParent {
                item_id,
                hierarchy_id,
            }))
        }
    }
}

// ============================================================================
// apply_one_deleted — single tombstone write
// ============================================================================

fn apply_one_deleted_locked(
    conn: &Connection,
    root_id: i64,
    root_path: &str,
    deleted: &DeletedItem,
    now: i64,
) -> Result<EventOutcome, FsPulseError> {
    if !path_is_within_root(&deleted.parent_path, root_path) {
        return Ok(EventOutcome::Skipped(SkipReason::OutsideRoot));
    }

    let path = join_full_path(&deleted.parent_path, &deleted.item_name);
    let current = lookup_current_item_locked(conn, root_id, &path, deleted.item_type)?;

    match current {
        None => {}                                  // Item never existed.
        Some(state) if state.is_deleted => {}       // Already deleted.
        Some(state) => write_tombstone_version_locked(conn, root_id, &state, now)?,
    }
    Ok(EventOutcome::Applied)
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
    item_version: i64,
    parent_item_id: Option<i64>,
    hierarchy_id: Vec<u8>,
    is_deleted: bool,
    access: Access,
    mod_date: Option<i64>,
    size: Option<i64>,
}

fn lookup_current_item_locked(
    conn: &Connection,
    root_id: i64,
    path: &str,
    item_type: ItemType,
) -> Result<Option<CurrentItem>, FsPulseError> {
    conn.query_row(
        "SELECT i.item_id, i.parent_item_id, i.hierarchy_id,
                iv.item_version, iv.is_deleted, iv.access, iv.mod_date, iv.size
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         WHERE i.root_id = ?1 AND i.item_path = ?2 AND i.item_type = ?3
         ORDER BY iv.item_version DESC
         LIMIT 1",
        params![root_id, path, item_type.as_i64()],
        |row| {
            Ok(CurrentItem {
                item_id: row.get(0)?,
                parent_item_id: row.get(1)?,
                hierarchy_id: row.get(2)?,
                item_version: row.get(3)?,
                is_deleted: row.get(4)?,
                access: Access::from_i64(row.get(5)?),
                mod_date: row.get(6)?,
                size: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(FsPulseError::DatabaseError)
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
        true,
        false,
        observed.access,
        observed.mod_date,
        observed.size,
    )?;

    Ok((item_id, new_hierarchy_id))
}

/// Extend last_seen_at on the current version of an unchanged item.
fn write_extend_last_seen_locked(
    conn: &Connection,
    item_id: i64,
    item_version: i64,
    now: i64,
) -> Result<(), FsPulseError> {
    conn.execute(
        "UPDATE item_versions
         SET last_seen_at = ?1
         WHERE item_id = ?2 AND item_version = ?3",
        params![now, item_id, item_version],
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
        is_added,
        false,
        observed.access,
        observed.mod_date,
        observed.size,
    )
}

/// Insert a tombstone item_versions row. Reuses the alive version's
/// access value (the user's last known intent for this path) and
/// writes NULL for mod_date and size since neither is meaningful for
/// a deleted item.
fn write_tombstone_version_locked(
    conn: &Connection,
    root_id: i64,
    current: &CurrentItem,
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
        false,
        true,
        current.access,
        None,
        None,
    )
}

/// The single item_versions INSERT used by every write helper. All
/// new versions stamp `first_seen_at = last_seen_at = now`.
#[allow(clippy::too_many_arguments)]
fn insert_item_version_locked(
    conn: &Connection,
    item_id: i64,
    item_version: i64,
    root_id: i64,
    parent_item_id: Option<i64>,
    hierarchy_id: &[u8],
    now: i64,
    is_added: bool,
    is_deleted: bool,
    access: Access,
    mod_date: Option<i64>,
    size: Option<i64>,
) -> Result<(), FsPulseError> {
    conn.execute(
        "INSERT INTO item_versions (
            item_id, item_version, root_id, parent_item_id, hierarchy_id,
            first_seen_at, last_seen_at,
            is_added, is_deleted, access, mod_date, size
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            item_id,
            item_version,
            root_id,
            parent_item_id,
            hierarchy_id,
            now,
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
    const SQL: &str = "
        SELECT 0 AS ord, (
            SELECT hierarchy_id FROM items
             WHERE root_id = ?1 AND parent_item_id IS ?2
               AND item_name COLLATE natural_path < ?3
             ORDER BY item_name COLLATE natural_path DESC
             LIMIT 1
        )
        UNION ALL
        SELECT 1, (
            SELECT hierarchy_id FROM items
             WHERE root_id = ?1 AND parent_item_id IS ?2
               AND item_name COLLATE natural_path > ?3
             ORDER BY item_name COLLATE natural_path ASC
             LIMIT 1
        )
        ORDER BY ord";

    let mut stmt = conn.prepare_cached(SQL)?;
    let mut rows = stmt.query(params![root_id, parent_item_id, name])?;
    let left_bytes: Option<Vec<u8>> = rows
        .next()?
        .ok_or_else(|| FsPulseError::Error("neighbor query missing left row".into()))?
        .get(0)?;
    let right_bytes: Option<Vec<u8>> = rows
        .next()?
        .ok_or_else(|| FsPulseError::Error("neighbor query missing right row".into()))?
        .get(0)?;

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

fn metadata_changed(current: &CurrentItem, observed: &ObservedItem) -> bool {
    if observed.item_type == ItemType::Directory {
        // Folders only get new versions for structural changes (added
        // / deleted) or access transitions. Mod_date and size are not
        // tracked for folders to avoid version churn from the watcher
        // when contained items change.
        current.access != observed.access
    } else {
        current.mod_date != observed.mod_date
            || current.size != observed.size
            || current.access != observed.access
    }
}

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
/// platform-native separators (matches how the scanner stores
/// `items.item_path`).
fn join_full_path(parent: &str, name: &str) -> String {
    Path::new(parent).join(name).to_string_lossy().into_owned()
}

fn path_is_within_root(parent_path: &str, root_path: &str) -> bool {
    Path::new(parent_path).starts_with(Path::new(root_path))
}
