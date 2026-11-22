//! Time-based reads: what a root looked like at a time, and what
//! changed between two times.
//!
//! Everything here is keyed by time, never by scan. A request names a
//! point in time, or a range, and gets back the state at the end of it
//! and, for a range, how each item differs between the two ends.
//!
//! # Which times are valid
//!
//! Changes to an item within one hour are folded into one version, so
//! the past is only known exactly at hour boundaries. [`TimeRange`]
//! therefore moves a range start, and an end that lies in the past, back
//! to the hour boundary at or before it. "Now" is the exception: nothing
//! exists after now, so the current state is always exact and the
//! current, unfinished hour can be the end of a range.
//!
//! # What "changed between A and B" means
//!
//! It is a comparison of the two ends, not a log. An item is:
//!
//! - **added** if it did not exist at A and exists at B,
//! - **deleted** if it existed at A and does not exist at B,
//! - **modified** if it exists at both and has a different version,
//! - **unchanged** if it exists at both with the same version,
//! - **transient** if it appeared and disappeared in between,
//! - **ignored** if it was present at A and has stopped being tracked
//!   by B because it matches an ignore pattern,
//! - **resumed** if it was ignored at A and is tracked and present
//!   again at B.
//!
//! Ignored and resumed are not deletions or additions and are never
//! counted as such.
//!
//! # What happened inside the range
//!
//! The comparison says what an item is at the end; it does not say
//! what was done to it. A file deleted and put back compares as
//! "modified". So each item that compares as added, modified or
//! deleted also carries [`Happened`]: whether it was added, modified
//! and deleted at least once inside the range. Counts for a folder or
//! a root count items by what happened to them, so one item can be in
//! more than one count. Transient, ignored and resumed items are not in
//! those counts.
//!
//! One hour is the finest grain: changes to an item within an hour are
//! one version, so a file created and then written to in the same hour
//! was added, not added and modified. (A large file being copied in is
//! seen several times as it grows; that is one addition.)
//!
//! Only the versions that began inside the range are needed to decide
//! this: the first one's `is_added` and `is_ignored` say what the item
//! was at A, and the last one's `is_deleted` and `is_ignored` say what
//! it is at B.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::FsPulseError;
use crate::hierarchy::HierarchyId;
use crate::item_identity::ItemType;
use crate::item_ops::BUCKET_SECS;

// ============================================================================
// Time ranges
// ============================================================================

/// A resolved point in time or range. Build one with [`TimeRange::resolve`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TimeRange {
    /// Start of the range, on an hour boundary. `None` for a point in
    /// time.
    pub from: Option<i64>,
    /// The time whose state is shown: now, or an hour boundary.
    pub to: i64,
    /// True when `to` is the present.
    pub to_is_now: bool,
    /// Versions that began before this are part of the state at `to`.
    #[serde(skip)]
    end: i64,
}

fn hour_start(time: i64) -> i64 {
    time - time.rem_euclid(BUCKET_SECS)
}

impl TimeRange {
    /// Resolve what a request asked for against the clock.
    ///
    /// - `to` missing, or not in the past: the present.
    /// - `to` in the past: the hour boundary at or before it.
    /// - `from` missing: a point in time.
    /// - `from` given: the hour boundary at or before it. A start that
    ///   is not before the end gives a point in time.
    pub fn resolve(from: Option<i64>, to: Option<i64>, now: i64) -> TimeRange {
        let (to, to_is_now, end) = match to {
            Some(to) if to < now => {
                let boundary = hour_start(to);
                (boundary, false, boundary)
            }
            // A version written this very second is part of "now".
            _ => (now, true, now + 1),
        };
        let from = from.map(hour_start).filter(|from| *from < to);
        TimeRange {
            from,
            to,
            to_is_now,
            end,
        }
    }

    /// The last second whose versions are part of the state at `to`,
    /// for queries written with an inclusive time.
    pub fn last_second(&self) -> i64 {
        self.end - 1
    }
}

// ============================================================================
// Change classification
// ============================================================================

/// How an item differs between the two ends of a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Added,
    Modified,
    Deleted,
    Unchanged,
    /// Appeared and disappeared inside the range, or was not being
    /// tracked at either end of it.
    Transient,
    /// Present at the start; no longer tracked at the end because it
    /// matches an ignore pattern. Not a deletion.
    Ignored,
    /// Ignored at the start; tracked and present again at the end. Not
    /// an addition.
    Resumed,
}

/// What an item was at one end of a range.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EndState {
    Present,
    /// Deleted, or never seen.
    Absent,
    /// Not tracked because of an ignore pattern.
    Ignored,
}

impl Change {
    /// Whether what happened to the item inside the range is reported
    /// and counted: only for an item that is listed for the range and
    /// is not there because of the ignore patterns.
    fn counts_what_happened(self) -> bool {
        matches!(self, Change::Added | Change::Modified | Change::Deleted)
    }

    /// Classify from the versions that began inside the range. `a` is
    /// `is_added * 2 + is_ignored` of the earliest and `d` is
    /// `is_deleted * 2 + is_ignored` of the latest, as the SQL packs
    /// them.
    ///
    /// The earliest version says what came before it: a version that is
    /// not an add followed a present one, an add followed a deletion
    /// (or nothing), a resume followed a stop. (A deletion recorded for
    /// an item that was stopped reads as "present before"; the item
    /// then shows as deleted in the range where it was found gone.)
    fn from_range_versions(a: i64, d: i64) -> Change {
        let at_start = match a {
            2 => EndState::Absent,
            3 => EndState::Ignored,
            _ => EndState::Present,
        };
        let at_end = match d {
            2 => EndState::Absent,
            3 => EndState::Ignored,
            _ => EndState::Present,
        };
        match (at_start, at_end) {
            (EndState::Present, EndState::Present) => Change::Modified,
            (EndState::Present, EndState::Absent) => Change::Deleted,
            (EndState::Present, EndState::Ignored) => Change::Ignored,
            (EndState::Absent, EndState::Present) => Change::Added,
            (EndState::Absent, EndState::Absent) => Change::Transient,
            (EndState::Absent, EndState::Ignored) => Change::Transient,
            (EndState::Ignored, EndState::Present) => Change::Resumed,
            (EndState::Ignored, EndState::Absent) => Change::Deleted,
            (EndState::Ignored, EndState::Ignored) => Change::Transient,
        }
    }
}

/// What was done to an item inside a range, each at least once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Happened {
    /// It appeared: created, or put back after a deletion.
    pub added: bool,
    /// It changed while it existed, in a later hour than it appeared.
    pub modified: bool,
    pub deleted: bool,
}

impl Happened {
    pub const ADDED: i64 = 1;
    pub const MODIFIED: i64 = 2;
    pub const DELETED: i64 = 4;
    pub const ANY: i64 = 7;

    /// From the bits the SQL computes (`h` in `changed_items!`). Only an
    /// item that compares as added, modified or deleted has anything
    /// recorded as happening to it; see the module docs.
    fn of(change: Change, bits: i64) -> Happened {
        if !change.counts_what_happened() {
            return Happened::default();
        }
        Happened {
            added: bits & Self::ADDED != 0,
            modified: bits & Self::MODIFIED != 0,
            deleted: bits & Self::DELETED != 0,
        }
    }

    pub fn any(self) -> bool {
        self.added || self.modified || self.deleted
    }
}

/// How many items beneath a folder, or in a root, had each kind of
/// change inside a range.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ChangeCounts {
    /// Items added, modified and deleted at least once. An item to
    /// which more than one of these happened is in each count.
    pub added: i64,
    pub modified: i64,
    pub deleted: i64,
    /// Items in at least one of the three counts above.
    pub changed: i64,
    pub transient: i64,
    /// Stopped being tracked because of an ignore pattern. Not in
    /// `deleted`.
    pub ignored: i64,
    /// Tracked again after being ignored. Not in `added`.
    pub resumed: i64,
}

impl ChangeCounts {
    fn record(&mut self, change: Change, happened: Happened) {
        match change {
            Change::Transient => self.transient += 1,
            Change::Ignored => self.ignored += 1,
            Change::Resumed => self.resumed += 1,
            Change::Added | Change::Modified | Change::Deleted | Change::Unchanged => {}
        }
        self.added += happened.added as i64;
        self.modified += happened.modified as i64;
        self.deleted += happened.deleted as i64;
        self.changed += happened.any() as i64;
    }
}

// ============================================================================
// Folder listing
// ============================================================================

/// One child of a folder, as of the end of a range.
#[derive(Debug, Clone, Serialize)]
pub struct TimelineItem {
    pub item_id: i64,
    pub item_path: String,
    pub item_name: String,
    #[serde(serialize_with = "serialize_item_type")]
    pub item_type: ItemType,
    pub has_validator: bool,
    /// True when the item does not exist at the end of the range. Such
    /// items are listed only for a range, and only if they existed at
    /// some point in it. Also true for an ignored item; see `is_ignored`.
    pub is_deleted: bool,
    /// True when the item is not tracked at the end of the range
    /// because it matches an ignore pattern. `first_seen_at` is then
    /// when tracking stopped. Listed only when ignored items are asked
    /// for.
    pub is_ignored: bool,
    /// When the version in effect began, and when it last changed.
    pub first_seen_at: i64,
    pub last_changed_at: i64,
    pub mod_date: Option<i64>,
    pub size: Option<i64>,
    pub val_state: Option<i64>,
    pub hash_state: Option<i64>,
    /// How the item differs between the two ends of the range. `None`
    /// for a point in time.
    pub change: Option<Change>,
    /// What was done to the item inside the range. `None` for a point
    /// in time.
    pub happened: Option<Happened>,
    /// Folders in a range only: what happened beneath this folder.
    pub descendants: Option<ChangeCounts>,
}

fn serialize_item_type<S: serde::Serializer>(
    item_type: &ItemType,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(item_type.short_name())
}

/// A folder's children with the range they were read for.
#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub range: TimeRange,
    pub items: Vec<TimelineItem>,
}

/// The folder whose children are listed.
struct Parent {
    /// `None` for the root directory, which is not an item.
    item_id: Option<i64>,
    /// Bounds of the folder's subtree in hierarchy order, both
    /// exclusive. `None` for the root directory, whose subtree is the
    /// whole root.
    subtree: Option<(Vec<u8>, Vec<u8>)>,
}

fn resolve_parent(
    conn: &Connection,
    root_id: i64,
    parent_path: &str,
) -> Result<Parent, FsPulseError> {
    let root_path: String = conn
        .query_row(
            "SELECT root_path FROM roots WHERE root_id = ?",
            [root_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| FsPulseError::Error(format!("Root {root_id} not found")))?;

    if Path::new(parent_path) == Path::new(&root_path) {
        return Ok(Parent {
            item_id: None,
            subtree: None,
        });
    }

    let trimmed = parent_path.trim_end_matches(std::path::MAIN_SEPARATOR);
    let (item_id, hierarchy_id): (i64, Vec<u8>) = conn
        .query_row(
            "SELECT item_id, hierarchy_id FROM items
              WHERE root_id = ?1 AND item_path = ?2 AND item_type = ?3",
            params![root_id, trimmed, ItemType::Directory.as_i64()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| FsPulseError::Error(format!("Folder not found: {parent_path}")))?;

    let end = HierarchyId::from_bytes(&hierarchy_id)
        .subtree_end()
        .ok_or_else(|| {
            FsPulseError::Error(format!("Folder has no position in the hierarchy: {parent_path}"))
        })?;
    Ok(Parent {
        item_id: Some(item_id),
        subtree: Some((hierarchy_id, end)),
    })
}

/// Children of a folder with the version in effect at the end of the
/// range.
/// Parameters: ?1 root_id, ?2 parent item_id (NULL for the root),
/// ?3 end of range (exclusive), ?4 start of range (or the end again for
/// a point in time, which lists nothing that is deleted), ?5 whether to
/// list ignored items. Ignored items are listed only when asked for,
/// whenever their tracking stopped: one rule for every range.
/// Uses idx_items_parent_name, which also gives the name order.
pub(crate) const CHILDREN_SQL: &str = "
    SELECT i.item_id, i.item_path, i.item_name, i.item_type, i.has_validator,
           i.hierarchy_id,
           iv.is_deleted, iv.first_seen_at, iv.last_changed_at,
           iv.mod_date, iv.size, iv.val_state,
           (SELECT hv.hash_state FROM hash_versions hv
             WHERE hv.item_id = iv.item_id AND hv.item_version = iv.item_version
               AND hv.first_seen_at < ?3
             ORDER BY hv.first_seen_at DESC LIMIT 1),
           iv.is_deleted AND iv.is_ignored
    FROM items i
    JOIN item_versions iv ON iv.item_id = i.item_id
       AND iv.item_version = (
           SELECT MAX(v.item_version) FROM item_versions v
            WHERE v.item_id = i.item_id AND v.first_seen_at < ?3
       )
    WHERE i.root_id = ?1 AND i.parent_item_id IS ?2
      AND (iv.is_deleted = 0
           OR (iv.is_ignored = 0 AND iv.first_seen_at >= ?4)
           OR (?5 AND iv.is_ignored = 1))
    ORDER BY i.item_name COLLATE natural_path";

/// The core of every range query: one row per item that has a version
/// which began inside the range, with what is needed to classify it.
/// Columns: item_id, hierarchy_id, `a` (the earliest such version's
/// `is_added * 2 + is_ignored`), `d` (the latest one's
/// `is_deleted * 2 + is_ignored`), `h` (what happened: the `Happened`
/// bits, one for each kind of version among them), last_version. See
/// `Change::from_range_versions` for what `a` and `d` mean. A version
/// written because of the ignore patterns is none of the three kinds;
/// a version that is neither an add nor a deletion is a modification.
/// Parameters: ?1 root_id, ?2 start, ?3 end (exclusive), and for the
/// folder form ?4 and ?5, the subtree's bounds in hierarchy order (both
/// exclusive).
///
/// Packing the version number above the flags makes MIN and MAX return
/// the flags of the earliest and latest version.
///
/// There are two forms because neither index suits every scope:
///
/// - `root`: reads every version in the root that began in the range,
///   from `idx_versions_root_time_hid`.
/// - `folder`: reads every version the subtree has ever had, from
///   `idx_versions_root_hid_time`, and keeps those in the range. For a
///   folder below the root that is far less than the root's activity in
///   the range. Rows come out in hierarchy order with no sort.
///
/// Both indexes carry `is_added`, `is_deleted` and `is_ignored`, so
/// neither form touches the table. An item has one hierarchy_id, so grouping by it
/// groups by item and orders the result depth-first.
macro_rules! changed_items {
    (root) => {
        "SELECT iv.item_id AS item_id, iv.hierarchy_id AS hierarchy_id,
                MIN(iv.item_version * 4 + iv.is_added * 2 + iv.is_ignored) & 3 AS a,
                MAX(iv.item_version * 4 + iv.is_deleted * 2 + iv.is_ignored) & 3 AS d,
                MAX(iv.is_added AND NOT iv.is_ignored)
                  + 2 * MAX(NOT iv.is_added AND NOT iv.is_deleted)
                  + 4 * MAX(iv.is_deleted AND NOT iv.is_ignored) AS h,
                MAX(iv.item_version) AS last_version
         FROM item_versions iv INDEXED BY idx_versions_root_time_hid
         WHERE iv.root_id = ?1
           AND iv.first_seen_at >= ?2 AND iv.first_seen_at < ?3
         GROUP BY iv.hierarchy_id"
    };
    (folder) => {
        "SELECT iv.item_id AS item_id, iv.hierarchy_id AS hierarchy_id,
                MIN(iv.item_version * 4 + iv.is_added * 2 + iv.is_ignored) & 3 AS a,
                MAX(iv.item_version * 4 + iv.is_deleted * 2 + iv.is_ignored) & 3 AS d,
                MAX(iv.is_added AND NOT iv.is_ignored)
                  + 2 * MAX(NOT iv.is_added AND NOT iv.is_deleted)
                  + 4 * MAX(iv.is_deleted AND NOT iv.is_ignored) AS h,
                MAX(iv.item_version) AS last_version
         FROM item_versions iv INDEXED BY idx_versions_root_hid_time
         WHERE iv.root_id = ?1
           AND iv.hierarchy_id > ?4 AND iv.hierarchy_id < ?5
           AND iv.first_seen_at >= ?2 AND iv.first_seen_at < ?3
         GROUP BY iv.hierarchy_id"
    };
}

pub(crate) const RANGE_CHANGES_IN_ROOT_SQL: &str = changed_items!(root);
pub(crate) const RANGE_CHANGES_IN_FOLDER_SQL: &str = changed_items!(folder);

/// List a folder's children as of the end of `range`. For a range, each
/// child says how it changed and each folder how its contents changed.
/// Transient items are left out unless `include_transient`. Items that
/// are not tracked at the end of the range because of an ignore pattern
/// are left out unless `include_ignored`, whether tracking stopped
/// inside the range or before it.
pub fn children(
    conn: &Connection,
    root_id: i64,
    parent_path: &str,
    range: TimeRange,
    include_transient: bool,
    include_ignored: bool,
) -> Result<Listing, FsPulseError> {
    let parent = resolve_parent(conn, root_id, parent_path)?;

    let mut stmt = conn.prepare_cached(CHILDREN_SQL)?;
    let rows = stmt
        .query_map(
            params![
                root_id,
                parent.item_id,
                range.end,
                range.from.unwrap_or(range.end),
                include_ignored
            ],
            |row| {
                Ok((
                    TimelineItem {
                        item_id: row.get(0)?,
                        item_path: row.get(1)?,
                        item_name: row.get(2)?,
                        item_type: ItemType::from_i64(row.get(3)?),
                        has_validator: row.get(4)?,
                        is_deleted: row.get(6)?,
                        is_ignored: row.get(13)?,
                        first_seen_at: row.get(7)?,
                        last_changed_at: row.get(8)?,
                        mod_date: row.get(9)?,
                        size: row.get(10)?,
                        val_state: row.get(11)?,
                        hash_state: row.get(12)?,
                        change: None,
                        happened: None,
                        descendants: None,
                    },
                    row.get::<_, Vec<u8>>(5)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    let (mut items, hierarchy_ids): (Vec<TimelineItem>, Vec<Vec<u8>>) = rows.into_iter().unzip();

    if let Some(from) = range.from {
        classify(conn, root_id, &parent, from, range.end, &mut items, &hierarchy_ids)?;
        if !include_transient {
            // An ignored item asked for by `include_ignored` stays even
            // if nothing about it changed net of the range.
            items.retain(|item| {
                item.change != Some(Change::Transient) || (include_ignored && item.is_ignored)
            });
        }
    }

    Ok(Listing { range, items })
}

/// Fill in `change` for each child and `descendants` for each folder
/// from one pass over the versions that began inside the range.
fn classify(
    conn: &Connection,
    root_id: i64,
    parent: &Parent,
    from: i64,
    end: i64,
    items: &mut [TimelineItem],
    hierarchy_ids: &[Vec<u8>],
) -> Result<(), FsPulseError> {
    for item in items.iter_mut() {
        item.change = Some(Change::Unchanged);
        item.happened = Some(Happened::default());
        if item.item_type == ItemType::Directory {
            item.descendants = Some(ChangeCounts::default());
        }
    }

    // Children in hierarchy order, so the child a changed item sits
    // under can be found by binary search.
    let mut by_position: Vec<usize> = (0..items.len()).collect();
    by_position.sort_by(|a, b| hierarchy_ids[*a].cmp(&hierarchy_ids[*b]));
    let positions: Vec<HierarchyId> = by_position
        .iter()
        .map(|index| HierarchyId::from_bytes(&hierarchy_ids[*index]))
        .collect();

    let mut stmt;
    let mut rows = match &parent.subtree {
        Some((lower, upper)) => {
            stmt = conn.prepare_cached(RANGE_CHANGES_IN_FOLDER_SQL)?;
            stmt.query(params![root_id, from, end, lower, upper])?
        }
        None => {
            stmt = conn.prepare_cached(RANGE_CHANGES_IN_ROOT_SQL)?;
            stmt.query(params![root_id, from, end])?
        }
    };

    while let Some(row) = rows.next()? {
        let hierarchy_id: Vec<u8> = row.get(1)?;
        let hierarchy_id = hierarchy_id.as_slice();
        let change = Change::from_range_versions(row.get(2)?, row.get(3)?);
        let happened = Happened::of(change, row.get(4)?);

        // The last child at or before this position is the child itself
        // or the child this item is somewhere beneath.
        let after = by_position.partition_point(|index| hierarchy_ids[*index].as_slice() <= hierarchy_id);
        let Some(slot) = after.checked_sub(1) else {
            continue;
        };
        let child = by_position[slot];

        if hierarchy_ids[child].as_slice() == hierarchy_id {
            items[child].change = Some(change);
            items[child].happened = Some(happened);
        } else if HierarchyId::from_bytes(hierarchy_id).is_descendant_of(&positions[slot]) {
            if let Some(counts) = items[child].descendants.as_mut() {
                counts.record(change, happened);
            }
        }
    }
    Ok(())
}

// ============================================================================
// Summary and flat list of changes
// ============================================================================

/// What happened in a root or one folder's subtree: see `ChangeCounts`.
/// Same parameters as `changed_items!`. `adm` is 1 for an item that
/// compares as added, modified or deleted, the only ones whose `h` is
/// counted.
macro_rules! summary_sql {
    ($scope:tt) => {
        concat!(
            "SELECT COALESCE(SUM(adm AND (h & 1) <> 0), 0),
                    COALESCE(SUM(adm AND (h & 2) <> 0), 0),
                    COALESCE(SUM(adm AND (h & 4) <> 0), 0),
                    COALESCE(SUM(adm AND h <> 0), 0),
                    COALESCE(SUM((a = 2 AND d >= 2) OR (a = 3 AND d = 3)), 0),
                    COALESCE(SUM(a < 2 AND d = 3), 0),
                    COALESCE(SUM(a = 3 AND d < 2), 0)
             FROM (SELECT a, d, h,
                          (a = 2 AND d < 2) OR (a < 2 AND d < 2) OR (a <> 2 AND d = 2) AS adm
                   FROM (",
            changed_items!($scope),
            "))"
        )
    };
}

pub(crate) const SUMMARY_IN_ROOT_SQL: &str = summary_sql!(root);
pub(crate) const SUMMARY_IN_FOLDER_SQL: &str = summary_sql!(folder);

/// One page of changed items in hierarchy (depth-first) order, each
/// with its path and its state at the end of the range.
/// Parameters: as `changed_items!`, plus which kinds to include (see
/// `Kinds`): ?6 `Kinds::compared`, ?7 limit, ?8 offset, ?9 the
/// combinations that compare as added, modified or deleted, ?10
/// `Kinds::happened`.
///
/// `a * 4 + d` numbers the sixteen combinations 0 to 15; `Change::mask`
/// has a bit for each one a kind covers. An item is included if its
/// combination is in ?6, or if it is in ?9 and something in ?10
/// happened to it. The page is cut before
/// the joins, so only the rows returned are looked up. `activity` is
/// the number of recorded changes to the item inside the range.
macro_rules! changes_page_sql {
    ($scope:tt) => {
        concat!(
            "SELECT i.item_id, i.item_path, i.item_name, i.item_type,
                    c.a, c.d, c.h, lv.size, lv.last_changed_at,
                    (SELECT SUM(v.change_count) FROM item_versions v
                      WHERE v.item_id = c.item_id
                        AND v.first_seen_at >= ?2 AND v.first_seen_at < ?3)
             FROM (",
            changed_items!($scope),
            "
                   HAVING ((?6 >> ((MIN(iv.item_version * 4 + iv.is_added * 2 + iv.is_ignored) & 3) * 4
                                   + (MAX(iv.item_version * 4 + iv.is_deleted * 2 + iv.is_ignored) & 3))) & 1)
                       OR (((?9 >> ((MIN(iv.item_version * 4 + iv.is_added * 2 + iv.is_ignored) & 3) * 4
                                    + (MAX(iv.item_version * 4 + iv.is_deleted * 2 + iv.is_ignored) & 3))) & 1)
                           AND ((MAX(iv.is_added AND NOT iv.is_ignored)
                                 + 2 * MAX(NOT iv.is_added AND NOT iv.is_deleted)
                                 + 4 * MAX(iv.is_deleted AND NOT iv.is_ignored)) & ?10) <> 0)
                   ORDER BY iv.hierarchy_id
                   LIMIT ?7 OFFSET ?8) c
             JOIN items i ON i.item_id = c.item_id
             JOIN item_versions lv ON lv.item_id = c.item_id AND lv.item_version = c.last_version
             ORDER BY c.hierarchy_id"
        )
    };
}

pub(crate) const CHANGES_PAGE_IN_ROOT_SQL: &str = changes_page_sql!(root);
pub(crate) const CHANGES_PAGE_IN_FOLDER_SQL: &str = changes_page_sql!(folder);

impl Change {
    /// Bits for this kind in the `kinds` filter of [`changes`]: one
    /// for every `a * 4 + d` combination (as the SQL computes it) that
    /// `from_range_versions` maps to this kind.
    pub fn mask(self) -> i64 {
        if self == Change::Unchanged {
            return 0;
        }
        let mut mask = 0;
        for a in 0..4 {
            for d in 0..4 {
                if Change::from_range_versions(a, d) == self {
                    mask |= 1 << (a * 4 + d);
                }
            }
        }
        mask
    }

    /// Every combination that compares as added, modified or deleted.
    fn added_modified_deleted_mask() -> i64 {
        Change::Added.mask() | Change::Modified.mask() | Change::Deleted.mask()
    }
}

/// Which items a list of changes includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Kinds {
    /// `Happened` bits: include items to which any of these happened.
    pub happened: i64,
    /// `Change::mask` bits: include items that compare as one of these
    /// kinds, whatever happened to them. For transient, ignored and
    /// resumed.
    pub compared: i64,
}

impl Kinds {
    /// Everything but transient: whatever was added, modified or
    /// deleted, and whatever the ignore patterns stopped or resumed.
    pub fn all() -> Kinds {
        Kinds {
            happened: Happened::ANY,
            compared: Change::Ignored.mask() | Change::Resumed.mask(),
        }
    }

    /// Add a kind by name. False if the name is not a kind.
    pub fn include(&mut self, kind: &str) -> bool {
        match kind {
            "added" => self.happened |= Happened::ADDED,
            "modified" => self.happened |= Happened::MODIFIED,
            "deleted" => self.happened |= Happened::DELETED,
            "transient" => self.compared |= Change::Transient.mask(),
            "ignored" => self.compared |= Change::Ignored.mask(),
            "resumed" => self.compared |= Change::Resumed.mask(),
            _ => return false,
        }
        true
    }
}

/// An item that changed inside a range.
#[derive(Debug, Clone, Serialize)]
pub struct ChangedItem {
    pub item_id: i64,
    pub item_path: String,
    pub item_name: String,
    #[serde(serialize_with = "serialize_item_type")]
    pub item_type: ItemType,
    /// How it differs between the two ends of the range.
    pub change: Change,
    /// What was done to it inside the range.
    pub happened: Happened,
    /// Size at the end of the range. `None` for folders and deleted
    /// items.
    pub size: Option<i64>,
    /// When the item last changed inside the range.
    pub last_changed_at: i64,
    /// How many changes to the item were recorded inside the range.
    pub activity: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub range: TimeRange,
    pub counts: ChangeCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangesPage {
    pub range: TimeRange,
    pub items: Vec<ChangedItem>,
}

/// What happened in a root, or beneath `folder_path`, inside `range`.
/// A point in time has no changes.
pub fn summary(
    conn: &Connection,
    root_id: i64,
    folder_path: Option<&str>,
    range: TimeRange,
) -> Result<Summary, FsPulseError> {
    let scope = resolve_scope(conn, root_id, folder_path)?;
    let Some(from) = range.from else {
        return Ok(Summary {
            range,
            counts: ChangeCounts::default(),
        });
    };

    let read = |row: &rusqlite::Row| {
        Ok(ChangeCounts {
            added: row.get(0)?,
            modified: row.get(1)?,
            deleted: row.get(2)?,
            changed: row.get(3)?,
            transient: row.get(4)?,
            ignored: row.get(5)?,
            resumed: row.get(6)?,
        })
    };
    let counts = match &scope.subtree {
        Some((lower, upper)) => conn
            .prepare_cached(SUMMARY_IN_FOLDER_SQL)?
            .query_row(params![root_id, from, range.end, lower, upper], read)?,
        None => conn
            .prepare_cached(SUMMARY_IN_ROOT_SQL)?
            .query_row(params![root_id, from, range.end], read)?,
    };
    Ok(Summary { range, counts })
}

/// One page of the items that changed in a root, or beneath
/// `folder_path`, in depth-first order. A point in time has no changes.
pub fn changes(
    conn: &Connection,
    root_id: i64,
    folder_path: Option<&str>,
    range: TimeRange,
    kinds: Kinds,
    limit: i64,
    offset: i64,
) -> Result<ChangesPage, FsPulseError> {
    let scope = resolve_scope(conn, root_id, folder_path)?;
    let Some(from) = range.from else {
        return Ok(ChangesPage {
            range,
            items: Vec::new(),
        });
    };

    let read = |row: &rusqlite::Row| {
        let change = Change::from_range_versions(row.get(4)?, row.get(5)?);
        Ok(ChangedItem {
            item_id: row.get(0)?,
            item_path: row.get(1)?,
            item_name: row.get(2)?,
            item_type: ItemType::from_i64(row.get(3)?),
            change,
            happened: Happened::of(change, row.get(6)?),
            size: row.get(7)?,
            last_changed_at: row.get(8)?,
            activity: row.get::<_, Option<i64>>(9)?.unwrap_or(0),
        })
    };
    let (sql, lower, upper) = match &scope.subtree {
        Some((lower, upper)) => (CHANGES_PAGE_IN_FOLDER_SQL, Some(lower), Some(upper)),
        None => (CHANGES_PAGE_IN_ROOT_SQL, None, None),
    };
    let items = conn
        .prepare_cached(sql)?
        .query_map(
            params![
                root_id,
                from,
                range.end,
                lower,
                upper,
                kinds.compared,
                limit,
                offset,
                Change::added_modified_deleted_mask(),
                kinds.happened
            ],
            read,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ChangesPage { range, items })
}

/// A root, or a folder in it when a path is given.
fn resolve_scope(
    conn: &Connection,
    root_id: i64,
    folder_path: Option<&str>,
) -> Result<Parent, FsPulseError> {
    match folder_path {
        Some(path) => resolve_parent(conn, root_id, path),
        None => {
            let exists = conn
                .query_row("SELECT 1 FROM roots WHERE root_id = ?", [root_id], |_| Ok(()))
                .optional()?
                .is_some();
            if !exists {
                return Err(FsPulseError::Error(format!("Root {root_id} not found")));
            }
            Ok(Parent {
                item_id: None,
                subtree: None,
            })
        }
    }
}

// ============================================================================
// One item
// ============================================================================

/// What happened to one item inside a range.
#[derive(Debug, Clone, Serialize)]
pub struct ItemInRange {
    /// The range as resolved. A version is inside it if it began at or
    /// after `from` and, unless the range ends now, before `to`.
    pub range: TimeRange,
    /// Changes to the item on disk recorded inside the range. Changes
    /// within one hour are one version, so this can be more than the
    /// number of versions.
    pub changes: i64,
    /// Everything that happened to it, whatever it is at the end. This
    /// is not cut down as it is for a listing: an item that came and
    /// went was added and deleted.
    pub happened: Happened,
    /// It came to match an ignore pattern and stopped being tracked.
    pub tracking_stopped: bool,
    /// It was tracked again after being ignored.
    pub tracking_resumed: bool,
}

/// Reads the item's versions by primary key.
/// Parameters: ?1 item_id, ?2 start, ?3 end (exclusive).
const ITEM_IN_RANGE_SQL: &str = "
    SELECT COALESCE(SUM(CASE WHEN is_ignored THEN 0 ELSE change_count END), 0),
           COALESCE(MAX(is_added AND NOT is_ignored), 0),
           COALESCE(MAX(NOT is_added AND NOT is_deleted), 0),
           COALESCE(MAX(is_deleted AND NOT is_ignored), 0),
           COALESCE(MAX(is_deleted AND is_ignored), 0),
           COALESCE(MAX(is_added AND is_ignored), 0)
    FROM item_versions
    WHERE item_id = ?1 AND first_seen_at >= ?2 AND first_seen_at < ?3";

/// What happened to one item inside `range`. A point in time has
/// nothing in it.
pub fn item_in_range(
    conn: &Connection,
    item_id: i64,
    range: TimeRange,
) -> Result<ItemInRange, FsPulseError> {
    let Some(from) = range.from else {
        return Ok(ItemInRange {
            range,
            changes: 0,
            happened: Happened::default(),
            tracking_stopped: false,
            tracking_resumed: false,
        });
    };
    let found = conn
        .prepare_cached(ITEM_IN_RANGE_SQL)?
        .query_row(params![item_id, from, range.end], |row| {
            Ok(ItemInRange {
                range,
                changes: row.get(0)?,
                happened: Happened {
                    added: row.get(1)?,
                    modified: row.get(2)?,
                    deleted: row.get(3)?,
                },
                tracking_stopped: row.get(4)?,
                tracking_resumed: row.get(5)?,
            })
        })?;
    Ok(found)
}

// ============================================================================
// Search
// ============================================================================

/// Items whose name contains a string, with the version in effect at
/// the end of the range and, for a range, how each changed.
/// Parameters: ?1 root_id, ?2 the string, ?3 end (exclusive), ?4 start
/// (or the end again for a point in time), ?5 limit, ?6 whether to
/// include ignored items whenever their tracking stopped.
/// Reads every item name in the root, in path order, until the limit
/// is reached.
pub(crate) const SEARCH_SQL: &str = "
    SELECT i.item_id, i.item_path, i.item_name, i.item_type, i.has_validator,
           iv.is_deleted, iv.first_seen_at, iv.last_changed_at,
           iv.mod_date, iv.size, iv.val_state,
           (SELECT hv.hash_state FROM hash_versions hv
             WHERE hv.item_id = iv.item_id AND hv.item_version = iv.item_version
               AND hv.first_seen_at < ?3
             ORDER BY hv.first_seen_at DESC LIMIT 1),
           (SELECT MIN(v.item_version * 4 + v.is_added * 2 + v.is_ignored) & 3 FROM item_versions v
             WHERE v.item_id = i.item_id
               AND v.first_seen_at >= ?4 AND v.first_seen_at < ?3),
           (SELECT MAX(v.item_version * 4 + v.is_deleted * 2 + v.is_ignored) & 3 FROM item_versions v
             WHERE v.item_id = i.item_id
               AND v.first_seen_at >= ?4 AND v.first_seen_at < ?3),
           iv.is_deleted AND iv.is_ignored,
           (SELECT MAX(v.is_added AND NOT v.is_ignored)
                   + 2 * MAX(NOT v.is_added AND NOT v.is_deleted)
                   + 4 * MAX(v.is_deleted AND NOT v.is_ignored) FROM item_versions v
             WHERE v.item_id = i.item_id
               AND v.first_seen_at >= ?4 AND v.first_seen_at < ?3)
    FROM items i
    JOIN item_versions iv ON iv.item_id = i.item_id
       AND iv.item_version = (
           SELECT MAX(v.item_version) FROM item_versions v
            WHERE v.item_id = i.item_id AND v.first_seen_at < ?3
       )
    WHERE i.root_id = ?1
      AND i.item_name LIKE '%' || ?2 || '%'
      AND (iv.is_deleted = 0
           OR (iv.is_ignored = 0 AND iv.first_seen_at >= ?4)
           OR (?6 AND iv.is_ignored = 1))
    ORDER BY i.item_path COLLATE natural_path
    LIMIT ?5";

/// Search a root by item name as of the end of `range`. For a range,
/// items deleted inside it are included and every result says how it
/// changed. Transient items are left out. Ignored items are left out
/// unless `include_ignored`.
pub fn search(
    conn: &Connection,
    root_id: i64,
    query: &str,
    range: TimeRange,
    limit: i64,
    include_ignored: bool,
) -> Result<Listing, FsPulseError> {
    let is_range = range.from.is_some();
    let mut stmt = conn.prepare_cached(SEARCH_SQL)?;
    let mut items = stmt
        .query_map(
            params![
                root_id,
                query,
                range.end,
                range.from.unwrap_or(range.end),
                limit,
                include_ignored
            ],
            |row| {
                let first_is_added: Option<i64> = row.get(12)?;
                let last_is_deleted: Option<i64> = row.get(13)?;
                let change = match (is_range, first_is_added, last_is_deleted) {
                    (false, _, _) => None,
                    (true, Some(a), Some(d)) => Some(Change::from_range_versions(a, d)),
                    (true, _, _) => Some(Change::Unchanged),
                };
                let bits: Option<i64> = row.get(15)?;
                let happened = change.map(|change| Happened::of(change, bits.unwrap_or(0)));
                Ok(TimelineItem {
                    item_id: row.get(0)?,
                    item_path: row.get(1)?,
                    item_name: row.get(2)?,
                    item_type: ItemType::from_i64(row.get(3)?),
                    has_validator: row.get(4)?,
                    is_deleted: row.get(5)?,
                    is_ignored: row.get(14)?,
                    first_seen_at: row.get(6)?,
                    last_changed_at: row.get(7)?,
                    mod_date: row.get(8)?,
                    size: row.get(9)?,
                    val_state: row.get(10)?,
                    hash_state: row.get(11)?,
                    change,
                    happened,
                    descendants: None,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    items.retain(|item| {
        item.change != Some(Change::Transient) || (include_ignored && item.is_ignored)
    });
    Ok(Listing { range, items })
}

// ============================================================================
// Activity
// ============================================================================

/// One hour in which something changed in a root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ActivityPoint {
    /// Start of the hour.
    pub time: i64,
    pub added: i64,
    pub modified: i64,
    pub deleted: i64,
    /// Totals as of the end of the hour.
    pub files: Option<i64>,
    pub folders: Option<i64>,
    pub size: Option<i64>,
}

/// A root's active hours between two times, oldest first. Hours with no
/// changes are absent; totals carry forward from the previous point.
/// Rows migrated from scans, which are not on hour boundaries, are
/// folded into the hour they fall in.
/// Parameters: ?1 root_id, ?2 from (inclusive), ?3 to (inclusive).
pub(crate) const ACTIVITY_SQL: &str = "
    SELECT taken_at - (taken_at % 3600),
           COALESCE(SUM(add_count), 0),
           COALESCE(SUM(modify_count), 0),
           COALESCE(SUM(delete_count), 0),
           MAX(taken_at), file_count, folder_count, total_size
    FROM snapshots
    WHERE root_id = ?1 AND taken_at >= ?2 AND taken_at <= ?3
    GROUP BY 1
    ORDER BY 1";

pub fn activity(
    conn: &Connection,
    root_id: i64,
    from: i64,
    to: i64,
) -> Result<Vec<ActivityPoint>, FsPulseError> {
    let mut stmt = conn.prepare_cached(ACTIVITY_SQL)?;
    let points = stmt
        .query_map(params![root_id, hour_start(from), to], |row| {
            Ok(ActivityPoint {
                time: row.get(0)?,
                added: row.get(1)?,
                modified: row.get(2)?,
                deleted: row.get(3)?,
                // With MAX(taken_at) in the select list, SQLite takes
                // these from the hour's latest row.
                files: row.get(5)?,
                folders: row.get(6)?,
                size: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::applier::{self, Hint};
    use crate::item_identity::VersionSource;
    use crate::item_ops::test_support::test_db_with_root;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Start of an hour.
    const H0: i64 = 1_700_000_000 - (1_700_000_000 % BUCKET_SECS);
    const HOUR: i64 = BUCKET_SECS;

    #[test]
    fn range_ends_in_the_past_move_back_to_the_hour() {
        let now = H0 + 5 * HOUR + 1800; // half past hour 5

        // Nothing given: the present, as a point in time.
        let present = TimeRange::resolve(None, None, now);
        assert_eq!((present.from, present.to, present.to_is_now), (None, now, true));
        assert_eq!(present.end, now + 1);

        // A past end moves back to its hour boundary.
        let past = TimeRange::resolve(None, Some(H0 + 3 * HOUR + 1200), now);
        assert_eq!((past.to, past.to_is_now, past.end), (H0 + 3 * HOUR, false, H0 + 3 * HOUR));

        // An end at or after now is the present.
        assert!(TimeRange::resolve(None, Some(now), now).to_is_now);
        assert!(TimeRange::resolve(None, Some(now + 999), now).to_is_now);

        // The open hour: 5:00 to now.
        let this_hour = TimeRange::resolve(Some(H0 + 5 * HOUR + 600), None, now);
        assert_eq!(this_hour.from, Some(H0 + 5 * HOUR));
        assert!(this_hour.to_is_now);

        // A sub-hour window in the past collapses to a point.
        let sub_hour = TimeRange::resolve(Some(H0 + 3 * HOUR + 600), Some(H0 + 3 * HOUR + 1200), now);
        assert_eq!((sub_hour.from, sub_hour.to), (None, H0 + 3 * HOUR));

        // A start after the end gives a point too.
        let backwards = TimeRange::resolve(Some(H0 + 4 * HOUR), Some(H0 + 2 * HOUR), now);
        assert_eq!(backwards.from, None);
    }

    struct Fixture {
        _dir: TempDir,
        root: PathBuf,
        conn: Connection,
        root_id: i64,
    }

    impl Fixture {
        fn new() -> Fixture {
            let dir = TempDir::new().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let (conn, root_id) = test_db_with_root(root.to_str().unwrap());
            Fixture {
                _dir: dir,
                root,
                conn,
                root_id,
            }
        }

        fn write(&self, rel: &str, contents: &str) {
            let path = self.root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }

        fn apply(&self, rels: &[&str], now: i64) {
            let hints: Vec<Hint> = rels
                .iter()
                .map(|rel| Hint {
                    root_id: self.root_id,
                    path: self.root.join(rel),
                    source: VersionSource::Watch,
                    confirmed: false,
                })
                .collect();
            applier::apply_hints_at(&self.conn, &hints, now).unwrap();
        }

        /// List a folder; `rel` is "" for the root.
        fn list(&self, rel: &str, from: Option<i64>, to: Option<i64>, now: i64) -> Vec<String> {
            self.list_with(rel, from, to, now, false)
        }

        fn list_with(
            &self,
            rel: &str,
            from: Option<i64>,
            to: Option<i64>,
            now: i64,
            include_transient: bool,
        ) -> Vec<String> {
            let parent = if rel.is_empty() {
                self.root.clone()
            } else {
                self.root.join(rel)
            };
            let range = TimeRange::resolve(from, to, now);
            children(&self.conn, self.root_id, parent.to_str().unwrap(), range, include_transient, false)
                .unwrap()
                .items
                .iter()
                .map(|item| {
                    let mut text = item.item_name.clone();
                    if let Some(change) = item.change {
                        text.push_str(&format!(" {change:?}").to_lowercase());
                    }
                    if let Some(c) = item.descendants {
                        text.push_str(&format!(
                            " [+{} ~{} -{} t{}]",
                            c.added, c.modified, c.deleted, c.transient
                        ));
                    }
                    text
                })
                .collect()
        }
    }

    /// Hour 0: a.txt, docs/one.txt, docs/sub/two.txt
    /// Hour 2: a.txt modified, docs/one.txt deleted, docs/new.txt and
    ///         b.txt added, docs/sub/two.txt modified
    /// Hour 4: tmp.txt added; a.txt modified again
    /// Hour 5: tmp.txt deleted
    fn history() -> Fixture {
        let fx = Fixture::new();
        fx.write("a.txt", "1");
        fx.write("docs/one.txt", "1");
        fx.write("docs/sub/two.txt", "2");
        fx.apply(&["a.txt", "docs/one.txt", "docs/sub/two.txt"], H0 + 60);

        fx.write("a.txt", "22");
        fs::remove_file(fx.root.join("docs/one.txt")).unwrap();
        fx.write("docs/new.txt", "n");
        fx.write("b.txt", "b");
        fx.write("docs/sub/two.txt", "2222");
        fx.apply(
            &["a.txt", "docs/one.txt", "docs/new.txt", "b.txt", "docs/sub/two.txt"],
            H0 + 2 * HOUR + 60,
        );

        fx.write("tmp.txt", "t");
        fx.write("a.txt", "333");
        fx.apply(&["tmp.txt", "a.txt"], H0 + 4 * HOUR + 60);

        fs::remove_file(fx.root.join("tmp.txt")).unwrap();
        fx.apply(&["tmp.txt"], H0 + 5 * HOUR + 60);
        fx
    }

    #[test]
    fn a_point_in_time_shows_what_existed_then() {
        let fx = history();
        let now = H0 + 10 * HOUR;

        assert_eq!(fx.list("", None, Some(H0 + HOUR), now), vec!["a.txt", "docs"]);
        assert_eq!(fx.list("docs", None, Some(H0 + HOUR), now), vec!["one.txt", "sub"]);
        assert_eq!(fx.list("docs", None, Some(H0 + 3 * HOUR), now), vec!["new.txt", "sub"]);
        assert_eq!(
            fx.list("", None, Some(H0 + 5 * HOUR), now),
            vec!["a.txt", "b.txt", "docs", "tmp.txt"]
        );
        assert_eq!(fx.list("", None, None, now), vec!["a.txt", "b.txt", "docs"]);
        // Before anything was recorded.
        assert!(fx.list("", None, Some(H0), now).is_empty());
    }

    #[test]
    fn a_range_compares_its_two_ends() {
        let fx = history();
        let now = H0 + 10 * HOUR;

        // Hour 1 to hour 3: the hour-2 changes.
        assert_eq!(
            fx.list("", Some(H0 + HOUR), Some(H0 + 3 * HOUR), now),
            vec!["a.txt modified", "b.txt added", "docs unchanged [+1 ~1 -1 t0]"]
        );
        assert_eq!(
            fx.list("docs", Some(H0 + HOUR), Some(H0 + 3 * HOUR), now),
            vec!["new.txt added", "one.txt deleted", "sub unchanged [+0 ~1 -0 t0]"]
        );

        // Hour 3 to now: a.txt modified; tmp.txt came and went.
        assert_eq!(
            fx.list("", Some(H0 + 3 * HOUR), None, now),
            vec!["a.txt modified", "b.txt unchanged", "docs unchanged [+0 ~0 -0 t0]"]
        );
        assert_eq!(
            fx.list_with("", Some(H0 + 3 * HOUR), None, now, true),
            vec![
                "a.txt modified",
                "b.txt unchanged",
                "docs unchanged [+0 ~0 -0 t0]",
                "tmp.txt transient"
            ]
        );

        // From before anything existed: everything alive now is added.
        assert_eq!(
            fx.list("", Some(H0 - HOUR), None, now),
            vec!["a.txt added", "b.txt added", "docs added [+3 ~1 -0 t1]"]
        );
    }

    #[test]
    fn the_open_hour_is_a_valid_range() {
        let fx = Fixture::new();
        fx.write("old.txt", "1");
        fx.apply(&["old.txt"], H0 + 60);

        // Half past hour 3: one file created at 3:05 and edited at 3:20,
        // one existing file edited at 3:15.
        fx.write("fresh.txt", "1");
        fx.apply(&["fresh.txt"], H0 + 3 * HOUR + 300);
        fx.write("old.txt", "22");
        fx.apply(&["old.txt"], H0 + 3 * HOUR + 900);
        fx.write("fresh.txt", "22");
        fx.apply(&["fresh.txt"], H0 + 3 * HOUR + 1200);

        let now = H0 + 3 * HOUR + 1800;
        assert_eq!(
            fx.list("", Some(H0 + 3 * HOUR), None, now),
            vec!["fresh.txt added", "old.txt modified"]
        );
    }

    #[test]
    fn a_deleted_folder_counts_what_was_in_it() {
        let fx = Fixture::new();
        fx.write("keep.txt", "k");
        fx.write("proj/a.rs", "a");
        fx.write("proj/src/b.rs", "b");
        fx.apply(&["keep.txt", "proj/a.rs", "proj/src/b.rs"], H0 + 60);

        fs::remove_dir_all(fx.root.join("proj")).unwrap();
        fx.apply(&["proj"], H0 + 2 * HOUR + 60);

        let now = H0 + 10 * HOUR;
        assert_eq!(
            fx.list("", Some(H0 + HOUR), None, now),
            vec!["keep.txt unchanged", "proj deleted [+0 ~0 -3 t0]"]
        );
        // The deleted folder can still be opened for that range.
        assert_eq!(
            fx.list("proj", Some(H0 + HOUR), None, now),
            vec!["a.rs deleted", "src deleted [+0 ~0 -1 t0]"]
        );
        // As a point in time, it is simply not there.
        assert_eq!(fx.list("", None, None, now), vec!["keep.txt"]);
    }

    #[test]
    fn changes_in_a_later_sibling_are_not_counted_under_an_earlier_folder() {
        let fx = Fixture::new();
        fx.write("a/x.txt", "x");
        fx.write("b/y.txt", "y");
        fx.write("c.txt", "c");
        fx.apply(&["a/x.txt", "b/y.txt", "c.txt"], H0 + 60);

        fx.write("b/z.txt", "z");
        fx.write("c.txt", "cc");
        fx.apply(&["b/z.txt", "c.txt"], H0 + 2 * HOUR + 60);

        let now = H0 + 10 * HOUR;
        assert_eq!(
            fx.list("", Some(H0 + HOUR), None, now),
            vec![
                "a unchanged [+0 ~0 -0 t0]",
                "b unchanged [+1 ~0 -0 t0]",
                "c.txt modified"
            ]
        );
        // Inside `a`, nothing from `b` or the root leaks in.
        assert_eq!(fx.list("a", Some(H0 + HOUR), None, now), vec!["x.txt unchanged"]);
    }

    #[test]
    fn an_unknown_folder_is_an_error() {
        let fx = history();
        let range = TimeRange::resolve(None, None, H0 + 10 * HOUR);
        let missing = fx.root.join("nope");
        assert!(children(&fx.conn, fx.root_id, missing.to_str().unwrap(), range, false, false).is_err());
        assert!(children(&fx.conn, 999, fx.root.to_str().unwrap(), range, false, false).is_err());
    }

    /// The kinds named, comma-separated.
    fn kinds(names: &str) -> Kinds {
        let mut kinds = Kinds::default();
        for name in names.split(',') {
            assert!(kinds.include(name), "{name} is not a kind");
        }
        kinds
    }

    fn changed(
        fx: &Fixture,
        folder: Option<&str>,
        from: i64,
        to: Option<i64>,
        kinds: Kinds,
        limit: i64,
        offset: i64,
    ) -> Vec<String> {
        let folder = folder.map(|rel| fx.root.join(rel).to_string_lossy().into_owned());
        let range = TimeRange::resolve(Some(from), to, H0 + 10 * HOUR);
        changes(&fx.conn, fx.root_id, folder.as_deref(), range, kinds, limit, offset)
            .unwrap()
            .items
            .iter()
            .map(|item| {
                let rel = Path::new(&item.item_path).strip_prefix(&fx.root).unwrap();
                format!("{} {:?} x{}", rel.display(), item.change, item.activity).to_lowercase()
            })
            .collect()
    }

    #[test]
    fn changes_are_listed_depth_first_and_can_be_filtered_and_paged() {
        let fx = history();
        let all = Kinds::all();

        // Hour 1 to hour 3.
        assert_eq!(
            changed(&fx, None, H0 + HOUR, Some(H0 + 3 * HOUR), all, 100, 0),
            vec![
                "a.txt modified x1",
                "b.txt added x1",
                "docs/new.txt added x1",
                "docs/one.txt deleted x1",
                "docs/sub/two.txt modified x1",
            ]
        );
        // The same, one folder only.
        assert_eq!(
            changed(&fx, Some("docs/sub"), H0 + HOUR, Some(H0 + 3 * HOUR), all, 100, 0),
            vec!["docs/sub/two.txt modified x1"]
        );
        // Only deletions.
        assert_eq!(
            changed(&fx, None, H0 + HOUR, Some(H0 + 3 * HOUR), kinds("deleted"), 100, 0),
            vec!["docs/one.txt deleted x1"]
        );
        // Second page of two.
        assert_eq!(
            changed(&fx, None, H0 + HOUR, Some(H0 + 3 * HOUR), all, 2, 2),
            vec!["docs/new.txt added x1", "docs/one.txt deleted x1"]
        );

        // Hour 1 to now: a.txt changed in two separate hours; tmp.txt is
        // transient and only listed when asked for.
        let to_now = changed(&fx, None, H0 + HOUR, None, all, 100, 0);
        assert!(to_now.contains(&"a.txt modified x2".to_string()));
        assert!(!to_now.iter().any(|line| line.starts_with("tmp.txt")));
        assert_eq!(
            changed(&fx, None, H0 + HOUR, None, kinds("transient"), 100, 0),
            vec!["tmp.txt transient x2"]
        );
    }

    /// Each child with what happened to it: "a.txt +~" is added and
    /// modified; "-" alone is nothing. Folders add their counts.
    fn happened_in(fx: &Fixture, rel: &str, from: i64, now: i64) -> Vec<String> {
        let parent = fx.root.join(rel);
        let range = TimeRange::resolve(Some(from), None, now);
        children(&fx.conn, fx.root_id, parent.to_str().unwrap(), range, false, false)
            .unwrap()
            .items
            .iter()
            .map(|item| {
                let h = item.happened.unwrap();
                let mut marks = String::new();
                if h.added {
                    marks.push('+');
                }
                if h.modified {
                    marks.push('~');
                }
                if h.deleted {
                    marks.push('x');
                }
                if marks.is_empty() {
                    marks.push('-');
                }
                match item.descendants {
                    Some(c) => format!(
                        "{} {marks} [+{} ~{} x{} of {}]",
                        item.item_name, c.added, c.modified, c.deleted, c.changed
                    ),
                    None => format!("{} {marks}", item.item_name),
                }
            })
            .collect()
    }

    #[test]
    fn an_item_reports_everything_that_happened_to_it() {
        let fx = history();
        // docs/one.txt, deleted in hour 2, is put back in hour 6.
        fx.write("docs/one.txt", "again");
        // A file created and written to twice more in the same hour.
        fx.write("c.txt", "1");
        fx.apply(&["docs/one.txt", "c.txt"], H0 + 6 * HOUR + 60);
        fx.write("c.txt", "22");
        fx.apply(&["c.txt"], H0 + 6 * HOUR + 600);
        fx.write("c.txt", "333");
        fx.apply(&["c.txt"], H0 + 6 * HOUR + 1200);
        let now = H0 + 10 * HOUR;

        // From before anything: a.txt was added and later modified.
        // c.txt's changes were all in the hour it appeared: added only.
        // tmp.txt came and went: not listed, not counted.
        assert_eq!(
            happened_in(&fx, "", H0 - HOUR, now),
            vec!["a.txt +~", "b.txt +", "c.txt +", "docs + [+4 ~1 x1 of 4]"]
        );
        // Beneath docs: new.txt, one.txt, sub and sub/two.txt were added;
        // two.txt was later modified; one.txt was deleted and put back.
        assert_eq!(
            happened_in(&fx, "docs", H0 - HOUR, now),
            vec!["new.txt +", "one.txt +x", "sub + [+1 ~1 x0 of 1]"]
        );

        // From hour 1, one.txt existed at both ends. It compares as
        // modified; what happened is that it was deleted and added.
        assert_eq!(
            happened_in(&fx, "docs", H0 + HOUR, now),
            vec!["new.txt +", "one.txt +x", "sub - [+0 ~1 x0 of 1]"]
        );
        let range = TimeRange::resolve(Some(H0 + HOUR), None, now);
        let counts = summary(&fx.conn, fx.root_id, None, range).unwrap().counts;
        assert_eq!(
            counts,
            // Added: b, c, new, one. Modified: a, two. Deleted: one.
            ChangeCounts { added: 4, modified: 2, deleted: 1, changed: 6, transient: 1, ..Default::default() }
        );

        // Filtering by what happened: one.txt is in both lists.
        let range_from = H0 + HOUR;
        assert_eq!(
            changed(&fx, None, range_from, None, kinds("deleted"), 100, 0),
            vec!["docs/one.txt modified x2"]
        );
        let added = changed(&fx, None, range_from, None, kinds("added"), 100, 0);
        assert_eq!(
            added,
            vec!["b.txt added x1", "c.txt added x3", "docs/new.txt added x1", "docs/one.txt modified x2"]
        );
        // Everything that changed, each once.
        assert_eq!(changed(&fx, None, range_from, None, Kinds::all(), 100, 0).len(), 6);
    }

    #[test]
    fn one_item_reports_what_happened_to_it_in_a_range() {
        let fx = history();
        let now = H0 + 10 * HOUR;
        let item = |rel: &str| -> i64 {
            fx.conn
                .query_row(
                    "SELECT item_id FROM items WHERE item_path = ?",
                    [fx.root.join(rel).to_str().unwrap()],
                    |row| row.get(0),
                )
                .unwrap()
        };
        let in_range = |rel: &str, from: i64, to: Option<i64>| {
            let range = TimeRange::resolve(Some(from), to, now);
            let found = item_in_range(&fx.conn, item(rel), range).unwrap();
            let h = found.happened;
            (found.changes, h.added, h.modified, h.deleted)
        };

        // a.txt: added in hour 0, modified in hours 2 and 4.
        assert_eq!(in_range("a.txt", H0 - HOUR, None), (3, true, true, false));
        assert_eq!(in_range("a.txt", H0 + HOUR, None), (2, false, true, false));
        assert_eq!(in_range("a.txt", H0 + HOUR, Some(H0 + 3 * HOUR)), (1, false, true, false));
        assert_eq!(in_range("a.txt", H0 + 5 * HOUR, None), (0, false, false, false));
        // tmp.txt came and went: for the item itself, that is reported.
        assert_eq!(in_range("tmp.txt", H0 + HOUR, None), (2, true, false, true));

        // A point in time has nothing in it.
        let point = TimeRange::resolve(None, None, now);
        assert_eq!(item_in_range(&fx.conn, item("a.txt"), point).unwrap().changes, 0);
    }

    #[test]
    fn summary_counts_match_the_list() {
        let fx = history();
        let now = H0 + 10 * HOUR;

        let range = TimeRange::resolve(Some(H0 + HOUR), Some(H0 + 3 * HOUR), now);
        let whole = summary(&fx.conn, fx.root_id, None, range).unwrap().counts;
        assert_eq!(
            whole,
            ChangeCounts { added: 2, modified: 2, deleted: 1, changed: 5, ..Default::default() }
        );

        let docs = fx.root.join("docs");
        let in_docs = summary(&fx.conn, fx.root_id, docs.to_str(), range).unwrap().counts;
        assert_eq!(
            in_docs,
            ChangeCounts { added: 1, modified: 1, deleted: 1, changed: 3, ..Default::default() }
        );

        let to_now = TimeRange::resolve(Some(H0 + 3 * HOUR), None, now);
        assert_eq!(
            summary(&fx.conn, fx.root_id, None, to_now).unwrap().counts,
            ChangeCounts { added: 0, modified: 1, deleted: 0, changed: 1, transient: 1, ..Default::default() }
        );

        // A point in time has no changes; an unknown root is an error.
        let point = TimeRange::resolve(None, None, now);
        assert_eq!(
            summary(&fx.conn, fx.root_id, None, point).unwrap().counts,
            ChangeCounts::default()
        );
        assert!(summary(&fx.conn, 999, None, range).is_err());
    }

    #[test]
    fn search_finds_names_as_of_a_time() {
        let fx = history();
        let now = H0 + 10 * HOUR;
        let find = |query: &str, from: Option<i64>, to: Option<i64>| -> Vec<String> {
            let range = TimeRange::resolve(from, to, now);
            search(&fx.conn, fx.root_id, query, range, 100, false)
                .unwrap()
                .items
                .iter()
                .map(|item| match item.change {
                    Some(change) => format!("{} {change:?}", item.item_name).to_lowercase(),
                    None => item.item_name.clone(),
                })
                .collect()
        };

        // At hour 1 only the first three files exist.
        assert_eq!(find(".txt", None, Some(H0 + HOUR)), vec!["a.txt", "one.txt", "two.txt"]);
        // Now: one.txt and tmp.txt are gone.
        assert_eq!(find(".txt", None, None), vec!["a.txt", "b.txt", "new.txt", "two.txt"]);
        // As a range, the deleted file is listed and everything is
        // classified; the transient file is not.
        assert_eq!(
            find(".txt", Some(H0 + HOUR), None),
            vec!["a.txt modified", "b.txt added", "new.txt added", "one.txt deleted", "two.txt modified"]
        );
        assert_eq!(find("sub", Some(H0 + HOUR), None), vec!["sub unchanged"]);
        assert!(find("zzz", None, None).is_empty());
    }

    fn set_patterns(fx: &Fixture, patterns: &[&str]) {
        let json = crate::ignore::IgnoreSet::to_stored_json(patterns).unwrap();
        fx.conn.execute("UPDATE roots SET ignore_patterns = ?1", [json]).unwrap();
    }

    /// `history()`, then:
    /// Hour 6: `docs` becomes ignored (docs, new.txt, sub, two.txt stop).
    /// Hour 8: the pattern is removed; docs/new.txt has been deleted
    ///         meanwhile, the rest resume.
    fn history_with_ignore() -> Fixture {
        let fx = history();
        set_patterns(&fx, &["docs"]);
        fx.apply(&["docs"], H0 + 6 * HOUR + 60);

        set_patterns(&fx, &[]);
        fs::remove_file(fx.root.join("docs/new.txt")).unwrap();
        fx.apply(
            &["docs", "docs/new.txt", "docs/sub", "docs/sub/two.txt"],
            H0 + 8 * HOUR + 60,
        );
        fx
    }

    fn counts(fx: &Fixture, from: i64, to: i64, now: i64) -> ChangeCounts {
        summary(&fx.conn, fx.root_id, None, TimeRange::resolve(Some(from), Some(to), now))
            .unwrap()
            .counts
    }

    #[test]
    fn every_combination_of_range_flags_belongs_to_exactly_one_kind() {
        let kinds = [
            Change::Added,
            Change::Modified,
            Change::Deleted,
            Change::Transient,
            Change::Ignored,
            Change::Resumed,
        ];
        let mut all = 0i64;
        for kind in kinds {
            assert_eq!(all & kind.mask(), 0, "{kind:?} overlaps another kind");
            all |= kind.mask();
        }
        assert_eq!(all, 0xFFFF);
    }

    #[test]
    fn ignoring_is_not_a_deletion_and_resuming_is_not_an_addition() {
        let fx = history_with_ignore();
        let now = H0 + 12 * HOUR;

        // Hour 5.x to 7: the ignore. Four items stop; nothing is deleted
        // except tmp.txt, which really was (hour 5).
        let ignore_hours = counts(&fx, H0 + 6 * HOUR, H0 + 7 * HOUR, now);
        assert_eq!((ignore_hours.ignored, ignore_hours.deleted, ignore_hours.added), (4, 0, 0));

        // Hour 7 to 9: the un-ignore. Three resume; new.txt, which was
        // found gone, is the one real deletion. Nothing is added.
        let resume_hours = counts(&fx, H0 + 7 * HOUR, H0 + 9 * HOUR, now);
        assert_eq!(
            (resume_hours.resumed, resume_hours.deleted, resume_hours.added),
            (3, 1, 0)
        );

        // An ignored item is not in a folder listing unless asked for,
        // even for the range in which it was ignored.
        assert_eq!(
            fx.list("", Some(H0 + 6 * HOUR), Some(H0 + 7 * HOUR), now),
            vec!["a.txt unchanged", "b.txt unchanged"]
        );
        let ignore_range = TimeRange::resolve(Some(H0 + 6 * HOUR), Some(H0 + 7 * HOUR), now);
        let asked: Vec<(String, Option<Change>)> =
            children(&fx.conn, fx.root_id, fx.root.to_str().unwrap(), ignore_range, false, true)
                .unwrap()
                .items
                .iter()
                .map(|item| (item.item_name.clone(), item.change))
                .collect();
        assert_eq!(asked.last().unwrap(), &("docs".to_string(), Some(Change::Ignored)));
        assert_eq!(
            fx.list("", Some(H0 + 7 * HOUR), Some(H0 + 9 * HOUR), now),
            vec!["a.txt unchanged", "b.txt unchanged", "docs resumed [+0 ~0 -1 t0]"]
        );

        // The snapshot counts agree.
        let (deleted, ignored, resumed): (i64, i64, i64) = fx
            .conn
            .query_row(
                "SELECT SUM(delete_count), SUM(ignored_count), SUM(resumed_count)
                 FROM snapshots WHERE taken_at >= ?1",
                [H0 + 6 * HOUR],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((deleted, ignored, resumed), (1, 4, 3));
    }

    #[test]
    fn ignored_items_are_listed_only_when_asked_for() {
        let fx = history_with_ignore();
        let now = H0 + 12 * HOUR;
        let root = fx.root.to_str().unwrap().to_string();
        let names = |range: TimeRange, include_ignored: bool| -> Vec<String> {
            children(&fx.conn, fx.root_id, &root, range, false, include_ignored)
                .unwrap()
                .items
                .iter()
                .map(|item| {
                    if item.is_ignored {
                        format!("{} (ignored)", item.item_name)
                    } else {
                        item.item_name.clone()
                    }
                })
                .collect()
        };

        // As of hour 8, docs is ignored: hidden by default.
        let at_8 = TimeRange::resolve(None, Some(H0 + 8 * HOUR), now);
        assert_eq!(names(at_8, false), vec!["a.txt", "b.txt"]);
        assert_eq!(names(at_8, true), vec!["a.txt", "b.txt", "docs (ignored)"]);

        // A range that begins after the ignore: the same.
        let after = TimeRange::resolve(Some(H0 + 7 * HOUR), Some(H0 + 8 * HOUR), now);
        assert_eq!(names(after, false), vec!["a.txt", "b.txt"]);
        assert_eq!(names(after, true), vec!["a.txt", "b.txt", "docs (ignored)"]);

        // Its contents can be listed too, as they were when it stopped.
        let docs = fx.root.join("docs");
        let inside: Vec<String> =
            children(&fx.conn, fx.root_id, docs.to_str().unwrap(), at_8, false, true)
                .unwrap()
                .items
                .iter()
                .map(|item| item.item_name.clone())
                .collect();
        assert_eq!(inside, vec!["new.txt", "sub"]);

        // Now it is tracked again and is listed normally either way.
        let current = TimeRange::resolve(None, None, now);
        assert_eq!(names(current, false), vec!["a.txt", "b.txt", "docs"]);

        // Search follows the same rule.
        let found = |include_ignored: bool| -> Vec<String> {
            search(&fx.conn, fx.root_id, "two", at_8, 100, include_ignored)
                .unwrap()
                .items
                .iter()
                .map(|item| item.item_name.clone())
                .collect()
        };
        assert!(found(false).is_empty());
        assert_eq!(found(true), vec!["two.txt"]);
    }

    #[test]
    fn the_changes_list_can_be_filtered_to_ignored_or_resumed() {
        let fx = history_with_ignore();
        let now = H0 + 12 * HOUR;
        let names = |from: i64, to: i64, kinds: Kinds| -> Vec<String> {
            let range = TimeRange::resolve(Some(from), Some(to), now);
            changes(&fx.conn, fx.root_id, None, range, kinds, 100, 0)
                .unwrap()
                .items
                .iter()
                .map(|item| format!("{} {:?}", item.item_name, item.change).to_lowercase())
                .collect()
        };

        assert_eq!(
            names(H0 + 6 * HOUR, H0 + 7 * HOUR, kinds("ignored")),
            vec!["docs ignored", "new.txt ignored", "sub ignored", "two.txt ignored"]
        );
        assert!(names(H0 + 6 * HOUR, H0 + 7 * HOUR, kinds("deleted")).is_empty());
        assert_eq!(
            names(H0 + 7 * HOUR, H0 + 9 * HOUR, kinds("resumed")),
            vec!["docs resumed", "sub resumed", "two.txt resumed"]
        );
        assert_eq!(
            names(H0 + 7 * HOUR, H0 + 9 * HOUR, kinds("deleted")),
            vec!["new.txt deleted"]
        );
        // Ignored and resumed inside one range: nothing happened on disk
        // to the items still there at both ends, so they are not listed
        // (though they compare as modified, having a different version).
        assert_eq!(
            names(H0 + 6 * HOUR, H0 + 9 * HOUR, Kinds::all()),
            vec!["new.txt deleted"]
        );
    }

    #[test]
    fn activity_lists_active_hours_with_totals() {
        let fx = history();
        let points = activity(&fx.conn, fx.root_id, H0, H0 + 10 * HOUR).unwrap();
        let summary: Vec<(i64, i64, i64, i64, Option<i64>)> = points
            .iter()
            .map(|p| ((p.time - H0) / HOUR, p.added, p.modified, p.deleted, p.files))
            .collect();
        assert_eq!(
            summary,
            vec![
                // docs and docs/sub are created along with the files.
                (0, 5, 0, 0, Some(3)),
                (2, 2, 2, 1, Some(4)),
                (4, 1, 1, 0, Some(5)),
                (5, 0, 0, 1, Some(4)),
            ]
        );

        // A narrower window.
        let later = activity(&fx.conn, fx.root_id, H0 + 3 * HOUR, H0 + 4 * HOUR + 10).unwrap();
        assert_eq!(later.len(), 1);
        assert_eq!(later[0].time, H0 + 4 * HOUR);
    }

    /// Run with FSPULSE_PLAN_TEST_DB pointing at a real database. Prints
    /// the plans and timings of the listing queries for the largest root
    /// over its busiest stretch.
    #[test]
    #[ignore]
    fn listing_plans_and_timings_on_a_real_database() {
        use crate::sort::compare_paths;
        use rusqlite::OpenFlags;
        use std::time::Instant;

        let path = std::env::var("FSPULSE_PLAN_TEST_DB").expect("set FSPULSE_PLAN_TEST_DB");
        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        conn.create_collation("natural_path", compare_paths).unwrap();

        for (label, sql, binds) in [
            ("children", CHILDREN_SQL, 5),
            ("range changes in root", RANGE_CHANGES_IN_ROOT_SQL, 3),
            ("range changes in folder", RANGE_CHANGES_IN_FOLDER_SQL, 5),
            ("changes page in root", CHANGES_PAGE_IN_ROOT_SQL, 10),
            ("changes page in folder", CHANGES_PAGE_IN_FOLDER_SQL, 10),
            ("search", SEARCH_SQL, 6),
            ("activity", ACTIVITY_SQL, 3),
        ] {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let zeros: Vec<i64> = vec![1; binds];
            let lines: Vec<String> = stmt
                .query_map(rusqlite::params_from_iter(zeros), |row| row.get::<_, String>(3))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            eprintln!("{label}:\n  {}\n", lines.join("\n  "));
        }

        let (root_id, root_path): (i64, String) = conn
            .query_row(
                "SELECT r.root_id, r.root_path FROM roots r
                  ORDER BY (SELECT COUNT(*) FROM items i WHERE i.root_id = r.root_id) DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let (oldest, newest): (i64, i64) = conn
            .query_row(
                "SELECT MIN(taken_at), MAX(taken_at) FROM snapshots WHERE root_id = ?",
                [root_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let now = newest + 2 * HOUR;

        let time = |label: &str, parent: &str, from: Option<i64>| {
            let range = TimeRange::resolve(from, None, now);
            let started = Instant::now();
            let listing = children(&conn, root_id, parent, range, false, false).unwrap();
            let changed: i64 = listing
                .items
                .iter()
                .filter_map(|item| item.descendants)
                .map(|c| c.added + c.modified + c.deleted)
                .sum();
            eprintln!(
                "{label}: {} children, {changed} changes beneath, {:?}",
                listing.items.len(),
                started.elapsed()
            );
        };

        time("root, now", &root_path, None);
        time("root, last 3 hours", &root_path, Some(newest - HOUR));
        time("root, last 30 days", &root_path, Some(newest - 30 * 24 * HOUR));
        time("root, all time", &root_path, Some(oldest - HOUR));
        let library = format!("{root_path}/Library");
        time("Library, last 3 hours", &library, Some(newest - HOUR));
        time("Library, all time", &library, Some(oldest - HOUR));
        let projects = format!("{root_path}/projects/fspulse/src");
        time("a small folder, last 3 hours", &projects, Some(newest - HOUR));
        time("a small folder, all time", &projects, Some(oldest - HOUR));

        for (label, folder, from) in [
            ("changes, root, last 3 hours", None, newest - HOUR),
            ("changes, root, all time", None, oldest - HOUR),
            ("changes, Library, last 3 hours", Some(library.as_str()), newest - HOUR),
            ("changes, small folder, all time", Some(projects.as_str()), oldest - HOUR),
        ] {
            let range = TimeRange::resolve(Some(from), None, now);
            let started = Instant::now();
            let counts = summary(&conn, root_id, folder, range).unwrap().counts;
            let summary_took = started.elapsed();
            let started = Instant::now();
            let first = changes(&conn, root_id, folder, range, Kinds::all(), 200, 0).unwrap();
            let first_took = started.elapsed();
            let started = Instant::now();
            let deep = changes(&conn, root_id, folder, range, Kinds::all(), 200, 100_000).unwrap();
            eprintln!(
                "{label}: {counts:?}; summary {summary_took:?}, first page ({}) {first_took:?}, page at 100k ({}) {:?}",
                first.items.len(),
                deep.items.len(),
                started.elapsed()
            );
        }

        for query in ["fspulse", ".rs", "zzzzqq"] {
            let range = TimeRange::resolve(Some(newest - HOUR), None, now);
            let started = Instant::now();
            let found = search(&conn, root_id, query, range, 500, false).unwrap();
            eprintln!("search '{query}': {} results, {:?}", found.items.len(), started.elapsed());
        }

        let started = Instant::now();
        let points = activity(&conn, root_id, oldest, now).unwrap();
        eprintln!("activity, all time: {} points, {:?}", points.len(), started.elapsed());
    }
}
