//! Snapshots: one row per root per hour in which something changed.
//!
//! A row holds how many items were added, modified and deleted in that
//! hour, and the root's totals (files, folders, bytes) as of the end of
//! it. Hours with no activity have no row; a reader carries the last
//! totals forward. The activity histogram and the Trends charts read
//! this table instead of scanning versions.
//!
//! Rows are written by the applier, in the same transaction as the item
//! writes they describe, so a snapshot never disagrees with the versions
//! behind it because of a crash. Totals are kept as running sums: each
//! batch adds its net change to the latest row's totals. A completed
//! reconcile walk recounts the root and corrects the totals if they have
//! drifted.
//!
//! Counts are items, not events. A file changed many times in an hour
//! is one version and counts as modified once.
//!
//! The integrity columns are NULL on rows written by the applier. A
//! completed integrity check counts the root's integrity posture and
//! writes it to the current hour's row (`record_integrity_locked`); a
//! reader carries the last posture forward, as with totals.

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::Database;
use crate::error::FsPulseError;
use crate::item_identity::ItemType;
use crate::item_ops::BUCKET_SECS;
use crate::utils::Utils;

/// What one batch of item writes changed in one root.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotDelta {
    /// Items that came into existence (including folders created
    /// because something was found inside them) or came back.
    pub added: i64,
    /// Items that got a new version because their metadata changed.
    /// Further changes folded into that version are not counted again.
    pub modified: i64,
    /// Items marked deleted.
    pub deleted: i64,
    /// Net change in alive files, alive folders and bytes in files.
    pub files: i64,
    pub folders: i64,
    pub size: i64,
}

impl SnapshotDelta {
    pub fn is_zero(&self) -> bool {
        *self == SnapshotDelta::default()
    }

    /// An item became alive.
    pub fn record_added(&mut self, item_type: ItemType, size: Option<i64>) {
        self.added += 1;
        self.count(item_type, size, 1);
    }

    /// An alive item was marked deleted.
    pub fn record_deleted(&mut self, item_type: ItemType, size: Option<i64>) {
        self.deleted += 1;
        self.count(item_type, size, -1);
    }

    /// Several alive items were marked deleted at once.
    pub fn record_deleted_many(&mut self, items: i64, totals: Totals) {
        self.deleted += items;
        self.files -= totals.files;
        self.folders -= totals.folders;
        self.size -= totals.size;
    }

    /// An alive item's recorded size changed. `new_version` is false
    /// when the change was folded into the item's current version.
    pub fn record_changed(
        &mut self,
        item_type: ItemType,
        old_size: Option<i64>,
        new_size: Option<i64>,
        new_version: bool,
    ) {
        if new_version {
            self.modified += 1;
        }
        if item_type == ItemType::File {
            self.size += new_size.unwrap_or(0) - old_size.unwrap_or(0);
        }
    }

    fn count(&mut self, item_type: ItemType, size: Option<i64>, sign: i64) {
        match item_type {
            ItemType::File => {
                self.files += sign;
                self.size += sign * size.unwrap_or(0);
            }
            ItemType::Directory => self.folders += sign,
            // Symlinks and other entries are tracked as items but are in
            // neither total, as on main.
            _ => {}
        }
    }
}

/// A root's totals: alive files, alive folders, and bytes in alive files.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Totals {
    pub files: i64,
    pub folders: i64,
    pub size: i64,
}

/// Totals of everything alive in a root. Parameter: ?1 root_id.
/// Reads every item of the root; used to seed a root's first snapshot
/// and to check the running totals after a reconcile walk.
pub(crate) const ALIVE_TOTALS_SQL: &str = "
    SELECT COALESCE(SUM(i.item_type = 0), 0),
           COALESCE(SUM(i.item_type = 1), 0),
           COALESCE(SUM(CASE WHEN i.item_type = 0 THEN iv.size END), 0)
    FROM items i
    JOIN item_versions iv ON iv.item_id = i.item_id
       AND iv.item_version = (
           SELECT MAX(iv2.item_version) FROM item_versions iv2
           WHERE iv2.item_id = i.item_id
       )
    WHERE i.root_id = ?1 AND iv.is_deleted = 0";

fn alive_totals_locked(conn: &Connection, root_id: i64) -> Result<Totals, FsPulseError> {
    Ok(conn.query_row(ALIVE_TOTALS_SQL, [root_id], |row| {
        Ok(Totals {
            files: row.get(0)?,
            folders: row.get(1)?,
            size: row.get(2)?,
        })
    })?)
}

/// Start of the hour bucket containing `now`.
fn bucket_start(now: i64) -> i64 {
    now - now.rem_euclid(BUCKET_SECS)
}

/// The most recent snapshot of a root: when, and its totals if it has
/// them.
fn latest_locked(
    conn: &Connection,
    root_id: i64,
) -> Result<Option<(i64, Option<Totals>)>, FsPulseError> {
    Ok(conn
        .prepare_cached(
            "SELECT taken_at, file_count, folder_count, total_size
               FROM snapshots WHERE root_id = ?1
              ORDER BY taken_at DESC LIMIT 1",
        )?
        .query_row([root_id], |row| {
            let files: Option<i64> = row.get(1)?;
            let folders: Option<i64> = row.get(2)?;
            let size: Option<i64> = row.get(3)?;
            let totals = match (files, folders, size) {
                (Some(files), Some(folders), Some(size)) => Some(Totals {
                    files,
                    folders,
                    size,
                }),
                _ => None,
            };
            Ok((row.get(0)?, totals))
        })
        .optional()?)
}

/// Write totals and add counts to a root's row for `taken_at`, creating
/// the row if needed.
fn upsert_locked(
    conn: &Connection,
    root_id: i64,
    taken_at: i64,
    totals: Totals,
    delta: &SnapshotDelta,
) -> Result<(), FsPulseError> {
    conn.prepare_cached(
        "INSERT INTO snapshots (
            root_id, taken_at, file_count, folder_count, total_size,
            add_count, modify_count, delete_count
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (root_id, taken_at) DO UPDATE SET
            file_count   = excluded.file_count,
            folder_count = excluded.folder_count,
            total_size   = excluded.total_size,
            add_count    = COALESCE(add_count, 0) + excluded.add_count,
            modify_count = COALESCE(modify_count, 0) + excluded.modify_count,
            delete_count = COALESCE(delete_count, 0) + excluded.delete_count",
    )?
    .execute(params![
        root_id,
        taken_at,
        totals.files,
        totals.folders,
        totals.size,
        delta.added,
        delta.modified,
        delta.deleted,
    ])?;
    Ok(())
}

/// The row a write at `now` belongs to: the current hour's, unless the
/// root already has a later row (a migrated scan from within this hour,
/// or the clock went back), in which case that one, so a root's rows
/// stay in time order.
fn target_row(now: i64, latest: Option<i64>) -> i64 {
    bucket_start(now).max(latest.unwrap_or(i64::MIN))
}

/// Record a batch's changes to a root. Must run in the transaction that
/// made the changes, after them.
pub(crate) fn apply_delta_locked(
    conn: &Connection,
    root_id: i64,
    delta: &SnapshotDelta,
    now: i64,
) -> Result<(), FsPulseError> {
    if delta.is_zero() {
        return Ok(());
    }

    let latest = latest_locked(conn, root_id)?;
    let totals = match latest {
        Some((_, Some(base))) => Totals {
            files: (base.files + delta.files).max(0),
            folders: (base.folders + delta.folders).max(0),
            size: (base.size + delta.size).max(0),
        },
        // No snapshot to build on. Count the root as it is now, which
        // already includes this batch.
        _ => alive_totals_locked(conn, root_id)?,
    };

    let taken_at = target_row(now, latest.map(|(taken_at, _)| taken_at));
    upsert_locked(conn, root_id, taken_at, totals, delta)
}

/// Recount a root and, if its latest snapshot's totals differ, write the
/// counted totals to the current hour's row. Returns true if a
/// correction was written. Must run under the write lock.
pub(crate) fn correct_totals_locked(
    conn: &Connection,
    root_id: i64,
    now: i64,
) -> Result<bool, FsPulseError> {
    let counted = alive_totals_locked(conn, root_id)?;
    let latest = latest_locked(conn, root_id)?;

    match latest {
        Some((_, Some(recorded))) if recorded == counted => return Ok(false),
        // A root with nothing in it and no history needs no row.
        None if counted == Totals::default() => return Ok(false),
        _ => {}
    }

    let taken_at = target_row(now, latest.map(|(taken_at, _)| taken_at));
    upsert_locked(conn, root_id, taken_at, counted, &SnapshotDelta::default())?;
    Ok(true)
}

/// Integrity posture of a root's alive files, counted from each file's
/// current version and that version's latest hash. Parameter: ?1 root_id.
/// Columns: validation unknown, valid, invalid, no validator; hash
/// unknown, baseline, suspect. Reads every item of the root.
pub(crate) const INTEGRITY_POSTURE_SQL: &str = "
    SELECT COALESCE(SUM(i.has_validator = 1 AND iv.val_state IS NULL), 0),
           COALESCE(SUM(iv.val_state = 1), 0),
           COALESCE(SUM(iv.val_state = 2), 0),
           COALESCE(SUM(i.has_validator = 0), 0),
           COALESCE(SUM(hv.hash_state IS NULL), 0),
           COALESCE(SUM(hv.hash_state = 1), 0),
           COALESCE(SUM(hv.hash_state = 2), 0)
    FROM items i
    JOIN item_versions iv ON iv.item_id = i.item_id
       AND iv.item_version = (
           SELECT MAX(iv2.item_version) FROM item_versions iv2
           WHERE iv2.item_id = i.item_id
       )
    LEFT JOIN hash_versions hv ON hv.item_id = iv.item_id
       AND hv.item_version = iv.item_version
       AND hv.first_seen_at = (
           SELECT MAX(hv2.first_seen_at) FROM hash_versions hv2
           WHERE hv2.item_id = iv.item_id AND hv2.item_version = iv.item_version
       )
    WHERE i.root_id = ?1 AND i.item_type = 0 AND iv.is_deleted = 0";

/// Count a root's integrity posture (see `INTEGRITY_POSTURE_SQL` for the
/// order). Takes seconds on a root with millions of files, so call it
/// outside the write lock.
pub(crate) fn count_integrity_posture(
    conn: &Connection,
    root_id: i64,
) -> Result<[i64; 7], FsPulseError> {
    Ok(conn.query_row(INTEGRITY_POSTURE_SQL, [root_id], |row| {
        Ok([
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
        ])
    })?)
}

/// Record the outcome of a completed integrity check: the root's
/// integrity posture as counted at its end, and how many problems the
/// check newly found. Goes on the current hour's row, which is created
/// (carrying the totals forward) if nothing else changed this hour. A
/// second check in the same hour replaces the posture and adds to the
/// new-problem counts. Must run under the write lock.
pub(crate) fn record_integrity_locked(
    conn: &Connection,
    root_id: i64,
    posture: &[i64; 7],
    new_val_invalid: i64,
    new_hash_suspect: i64,
    now: i64,
) -> Result<(), FsPulseError> {

    let latest = latest_locked(conn, root_id)?;
    let totals = match latest {
        Some((_, Some(totals))) => totals,
        _ => alive_totals_locked(conn, root_id)?,
    };
    let taken_at = target_row(now, latest.map(|(taken_at, _)| taken_at));

    conn.execute(
        "INSERT INTO snapshots (
            root_id, taken_at, file_count, folder_count, total_size,
            add_count, modify_count, delete_count,
            val_unknown_count, val_valid_count, val_invalid_count, val_no_validator_count,
            hash_unknown_count, hash_baseline_count, hash_suspect_count,
            new_val_invalid_count, new_hash_suspect_count
         ) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT (root_id, taken_at) DO UPDATE SET
            val_unknown_count      = excluded.val_unknown_count,
            val_valid_count        = excluded.val_valid_count,
            val_invalid_count      = excluded.val_invalid_count,
            val_no_validator_count = excluded.val_no_validator_count,
            hash_unknown_count     = excluded.hash_unknown_count,
            hash_baseline_count    = excluded.hash_baseline_count,
            hash_suspect_count     = excluded.hash_suspect_count,
            new_val_invalid_count  = COALESCE(new_val_invalid_count, 0)
                                     + excluded.new_val_invalid_count,
            new_hash_suspect_count = COALESCE(new_hash_suspect_count, 0)
                                     + excluded.new_hash_suspect_count",
        params![
            root_id,
            taken_at,
            totals.files,
            totals.folders,
            totals.size,
            posture[0],
            posture[1],
            posture[2],
            posture[3],
            posture[4],
            posture[5],
            posture[6],
            new_val_invalid,
            new_hash_suspect,
        ],
    )?;
    Ok(())
}

/// Recount a root after a completed reconcile walk and correct its
/// snapshot totals if they have drifted. Returns true if they had.
pub fn correct_totals(root_id: i64) -> Result<bool, FsPulseError> {
    let conn = Database::get_connection()?;
    Database::immediate_transaction(&conn, |tx| {
        correct_totals_locked(tx, root_id, Utils::now_secs())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::applier::{self, Hint};
    use crate::item_identity::VersionSource;
    use crate::item_ops::test_support::{count, test_db_with_root};
    use rusqlite::Connection;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Start of an hour, and offsets into it and the next.
    const HOUR: i64 = 1_700_000_000 - (1_700_000_000 % BUCKET_SECS);

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

        /// Apply hints for the given paths at `now`, as the watcher would.
        fn touch(&self, rels: &[&str], now: i64) {
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

        /// (taken_at offset from HOUR, files, folders, size, adds, modifies, deletes)
        fn rows(&self) -> Vec<(i64, i64, i64, i64, i64, i64, i64)> {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT taken_at, file_count, folder_count, total_size,
                            add_count, modify_count, delete_count
                       FROM snapshots ORDER BY taken_at",
                )
                .unwrap();
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)? - HOUR,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
        }

        fn counted(&self) -> Totals {
            alive_totals_locked(&self.conn, self.root_id).unwrap()
        }
    }

    #[test]
    fn one_row_per_active_hour_with_running_totals() {
        let fx = Fixture::new();

        // Hour 0: two files in a new folder. The folder is created
        // because a file was found in it, and counts as added.
        fx.write("docs/a.txt", "aaaa");
        fx.write("docs/b.txt", "bb");
        fx.touch(&["docs/a.txt", "docs/b.txt"], HOUR + 60);
        assert_eq!(fx.rows(), vec![(0, 2, 1, 6, 3, 0, 0)]);

        // Same hour, a second batch: one more file. Same row.
        fx.write("c.txt", "c");
        fx.touch(&["c.txt"], HOUR + 120);
        assert_eq!(fx.rows(), vec![(0, 3, 1, 7, 4, 0, 0)]);

        // Hour 2 (hour 1 is quiet and gets no row): grow one file,
        // delete another.
        fx.write("docs/a.txt", "aaaaaaaaaa");
        fs::remove_file(fx.root.join("c.txt")).unwrap();
        fx.touch(&["docs/a.txt", "c.txt"], HOUR + 2 * BUCKET_SECS + 5);
        assert_eq!(
            fx.rows(),
            vec![(0, 3, 1, 7, 4, 0, 0), (2 * BUCKET_SECS, 2, 1, 12, 0, 1, 1)]
        );
        assert_eq!(fx.counted(), Totals { files: 2, folders: 1, size: 12 });
    }

    #[test]
    fn repeated_changes_in_an_hour_count_once_but_totals_follow() {
        let fx = Fixture::new();
        fx.write("a.txt", "1");
        fx.touch(&["a.txt"], HOUR + 10);

        // Next hour: three changes to the same file.
        let next = HOUR + BUCKET_SECS;
        for (n, contents) in ["22", "333", "4444"].iter().enumerate() {
            fx.write("a.txt", contents);
            fx.touch(&["a.txt"], next + 10 * n as i64);
        }

        assert_eq!(count(&fx.conn, "SELECT COUNT(*) FROM item_versions"), 2);
        assert_eq!(
            fx.rows(),
            vec![(0, 1, 0, 1, 1, 0, 0), (BUCKET_SECS, 1, 0, 4, 0, 1, 0)]
        );
    }

    #[test]
    fn deleting_a_folder_removes_its_whole_subtree_from_the_totals() {
        let fx = Fixture::new();
        fx.write("keep.txt", "k");
        fx.write("proj/src/main.rs", "fn main() {}");
        fx.write("proj/src/lib.rs", "pub fn f() {}");
        fx.write("proj/readme.md", "hi");
        fx.touch(
            &["keep.txt", "proj/src/main.rs", "proj/src/lib.rs", "proj/readme.md"],
            HOUR + 1,
        );
        assert_eq!(fx.counted(), Totals { files: 4, folders: 2, size: 28 });

        fs::remove_dir_all(fx.root.join("proj")).unwrap();
        fx.touch(&["proj"], HOUR + BUCKET_SECS + 1);

        let rows = fx.rows();
        assert_eq!(rows[1], (BUCKET_SECS, 1, 0, 1, 0, 0, 5));
        assert_eq!(fx.counted(), Totals { files: 1, folders: 0, size: 1 });
    }

    #[test]
    fn a_batch_that_changes_nothing_writes_no_row() {
        let fx = Fixture::new();
        fx.write("a.txt", "1");
        fx.touch(&["a.txt"], HOUR + 10);
        fx.touch(&["a.txt", "never-existed.txt"], HOUR + BUCKET_SECS + 10);
        assert_eq!(fx.rows().len(), 1);
    }

    #[test]
    fn totals_continue_from_a_migrated_row_and_drift_is_corrected() {
        let fx = Fixture::new();
        fx.write("a.txt", "12345");
        fx.touch(&["a.txt"], HOUR + 10);

        // Make the recorded totals wrong, as drift or an old scan's
        // different way of counting would.
        fx.conn
            .execute("UPDATE snapshots SET file_count = 10, total_size = 999", [])
            .unwrap();

        // The next batch builds on the recorded totals, right or wrong.
        fx.write("b.txt", "1");
        fx.touch(&["b.txt"], HOUR + BUCKET_SECS + 10);
        assert_eq!(fx.rows()[1], (BUCKET_SECS, 11, 0, 1000, 1, 0, 0));

        // A recount fixes the totals and leaves the change counts alone.
        let later = HOUR + BUCKET_SECS + 600;
        assert!(correct_totals_locked(&fx.conn, fx.root_id, later).unwrap());
        assert_eq!(fx.rows()[1], (BUCKET_SECS, 2, 0, 6, 1, 0, 0));

        // Nothing to correct the second time, and no row is added.
        let much_later = HOUR + 5 * BUCKET_SECS;
        assert!(!correct_totals_locked(&fx.conn, fx.root_id, much_later).unwrap());
        assert_eq!(fx.rows().len(), 2);
    }

    #[test]
    fn a_row_later_than_now_is_reused_so_rows_stay_in_order() {
        let fx = Fixture::new();
        // A migrated scan row from later in the current hour.
        fx.conn
            .execute(
                "INSERT INTO snapshots (root_id, taken_at, file_count, folder_count, total_size,
                                        add_count, modify_count, delete_count)
                 VALUES (?1, ?2, 0, 0, 0, 0, 0, 0)",
                params![fx.root_id, HOUR + 1200],
            )
            .unwrap();

        fx.write("a.txt", "1");
        fx.touch(&["a.txt"], HOUR + 1500);
        assert_eq!(fx.rows(), vec![(1200, 1, 0, 1, 1, 0, 0)]);
    }

    #[test]
    fn an_empty_root_with_no_history_gets_no_row() {
        let fx = Fixture::new();
        assert!(!correct_totals_locked(&fx.conn, fx.root_id, HOUR).unwrap());
        assert!(fx.rows().is_empty());
    }
    #[test]
    fn a_completed_integrity_check_records_posture_on_the_current_hours_row() {
        let f = Fixture::new();
        f.write("a.flac", "aaaa");
        f.write("b.flac", "bb");
        f.write("notes.txt", "n");
        f.touch(&["a.flac", "b.flac", "notes.txt"], HOUR + 10);

        // a.flac: validated and hashed. b.flac: invalid, hashed twice,
        // the second time with a different result. notes.txt: nothing.
        let id = |name: &str| -> i64 {
            f.conn
                .query_row("SELECT item_id FROM items WHERE item_name = ?1", [name], |r| r.get(0))
                .unwrap()
        };
        let (a, b, notes) = (id("a.flac"), id("b.flac"), id("notes.txt"));
        f.conn
            .execute("UPDATE items SET has_validator = (item_id <> ?1)", [notes])
            .unwrap();
        f.conn
            .execute("UPDATE item_versions SET val_state = 1 WHERE item_id = ?1", [a])
            .unwrap();
        f.conn
            .execute("UPDATE item_versions SET val_state = 2 WHERE item_id = ?1", [b])
            .unwrap();
        for (item, at, state) in [(a, 20, 1), (b, 20, 1), (b, 30, 2)] {
            f.conn
                .execute(
                    "INSERT INTO hash_versions
                        (item_id, item_version, first_seen_at, last_seen_at, file_hash, hash_state)
                     VALUES (?1, 1, ?2, ?2, x'00', ?3)",
                    params![item, HOUR + at, state],
                )
                .unwrap();
        }

        let integrity = |conn: &Connection| -> Vec<(i64, Vec<Option<i64>>)> {
            let mut stmt = conn
                .prepare(
                    "SELECT taken_at, file_count, add_count,
                            val_unknown_count, val_valid_count, val_invalid_count,
                            val_no_validator_count, hash_unknown_count,
                            hash_baseline_count, hash_suspect_count,
                            new_val_invalid_count, new_hash_suspect_count
                       FROM snapshots ORDER BY taken_at",
                )
                .unwrap();
            let rows = stmt
                .query_map([], |row| {
                    let taken_at: i64 = row.get(0)?;
                    let values = (1..12).map(|i| row.get(i)).collect::<Result<_, _>>()?;
                    Ok((taken_at - HOUR, values))
                })
                .unwrap();
            rows.map(Result::unwrap).collect()
        };
        let some = |values: &[i64]| values.iter().map(|v| Some(*v)).collect::<Vec<_>>();

        let posture = count_integrity_posture(&f.conn, f.root_id).unwrap();
        assert_eq!(posture, [0, 1, 1, 1, 1, 1, 1]);

        // In the hour the files arrived: posture lands on that hour's
        // row beside its change counts.
        record_integrity_locked(&f.conn, f.root_id, &posture, 1, 1, HOUR + 40).unwrap();
        assert_eq!(
            integrity(&f.conn),
            vec![(0, some(&[3, 3, 0, 1, 1, 1, 1, 1, 1, 1, 1]))]
        );

        // A second check in the same hour replaces the posture and adds
        // to what was newly found.
        record_integrity_locked(&f.conn, f.root_id, &posture, 0, 2, HOUR + 50).unwrap();
        assert_eq!(
            integrity(&f.conn),
            vec![(0, some(&[3, 3, 0, 1, 1, 1, 1, 1, 1, 1, 3]))]
        );

        // In a later hour with no changes: a new row carrying the
        // totals forward, with no changes counted.
        record_integrity_locked(&f.conn, f.root_id, &posture, 0, 0, HOUR + BUCKET_SECS + 5).unwrap();
        let rows = integrity(&f.conn);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[1],
            (BUCKET_SECS, some(&[3, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0]))
        );

        // The applier's rows leave the integrity columns alone.
        f.write("c.flac", "c");
        f.touch(&["c.flac"], HOUR + BUCKET_SECS + 60);
        let rows = integrity(&f.conn);
        assert_eq!(rows[1].1[0], Some(4));
        assert_eq!(rows[1].1[4], Some(1));
    }
}
