//! Reconcile walk: find the paths where the filesystem and the database
//! disagree.
//!
//! The walk visits every directory under a root. For each one it lists
//! the directory on disk, loads the same directory's children from the
//! database, and compares the two in memory. It reports a path only when
//! they differ: something new, something changed, or something recorded
//! as alive that is no longer there. It writes nothing itself; the
//! caller forwards the reported paths to the applier.
//!
//! A walk over a tree that matches the database therefore produces no
//! hints and no writes, however large the tree is.
//!
//! This is what catches changes the watcher missed: events dropped on
//! overflow, changes made while fsPulse was not running, and roots the
//! platform cannot watch at all.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{params, Connection};

use crate::error::FsPulseError;
use crate::ignore::IgnoreSet;
use crate::item_identity::{Access, ItemType, VersionSource};
use crate::item_ops::ObservedItem;

/// Children of one directory with their latest versions.
/// Parameters: ?1 root_id, ?2 parent item_id (NULL for the root).
/// Uses idx_items_parent_name.
pub(crate) const RECORDED_CHILDREN_SQL: &str = "
    SELECT i.item_id, i.item_name, i.item_type,
           iv.is_deleted, iv.access, iv.mod_date, iv.size
    FROM items i
    JOIN item_versions iv ON iv.item_id = i.item_id
       AND iv.item_version = (
           SELECT MAX(iv2.item_version) FROM item_versions iv2
           WHERE iv2.item_id = i.item_id
       )
    WHERE i.root_id = ?1 AND i.parent_item_id IS ?2";

/// Items alive under a directory, at any depth.
/// Parameters: ?1 root_id, ?2 the directory's item_id.
/// Same shape as `item_ops::TOMBSTONE_DESCENDANTS_SQL`, which writes the
/// tombstones this counts in advance.
pub(crate) const ALIVE_DESCENDANTS_SQL: &str = "
    WITH RECURSIVE sub(item_id) AS (
        SELECT item_id FROM items
         WHERE root_id = ?1 AND parent_item_id = ?2
        UNION ALL
        SELECT i.item_id FROM sub s
         CROSS JOIN items i ON i.root_id = ?1 AND i.parent_item_id = s.item_id
    )
    SELECT COUNT(*)
    FROM sub
    CROSS JOIN item_versions iv ON iv.item_id = sub.item_id
       AND iv.item_version = (
           SELECT MAX(iv2.item_version) FROM item_versions iv2
           WHERE iv2.item_id = sub.item_id
       )
    WHERE iv.is_deleted = 0";

/// Why a path is being reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difference {
    /// The path is on disk and is new, changed, or of another type than
    /// recorded.
    Changed,
    /// The database records the path as alive and it is not on disk (or
    /// is now ignored). Applying this marks the item, and everything
    /// under it, as deleted.
    Missing,
}

/// Counters for one walk.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WalkStats {
    /// Directory entries examined (files, folders, symlinks, other).
    pub entries: u64,
    /// Directories listed, including the root.
    pub dirs: u64,
    /// Paths reported as differing from the database.
    pub hints: u64,
    /// Directories where at least one entry could not be examined
    /// because of an I/O error. Nothing in such a directory is reported
    /// as gone, so a deletion there waits for a later walk.
    pub incomplete_dirs: u64,
    /// Items that would be marked deleted if every `Missing` path were
    /// applied: each missing path plus everything alive beneath it.
    pub missing_items: u64,
}

/// Walk `root_path` and call `on_hint` with every path whose state on
/// disk differs from what the database records for `root_id`.
///
/// `conn` is used for reads only. `on_progress` is called after each
/// directory. The walk stops with `FsPulseError::TaskInterrupted` when
/// `interrupt` is set.
///
/// Entries matching the root's ignore patterns are treated as absent.
///
/// Refuses to run (returns `FsPulseError::RootEmpty`, reports nothing)
/// when the root directory is empty on disk but the database records
/// live items in it. That is what an unmounted volume looks like, and
/// reporting every item as gone would tombstone the whole root.
/// `allow_empty_root` turns the refusal off, for a walk the user has
/// confirmed.
pub fn find_differences(
    conn: &Connection,
    root_id: i64,
    root_path: &Path,
    allow_empty_root: bool,
    interrupt: &AtomicBool,
    on_hint: &mut dyn FnMut(PathBuf, Difference) -> Result<(), FsPulseError>,
    on_progress: &mut dyn FnMut(&WalkStats),
) -> Result<WalkStats, FsPulseError> {
    let entries = match fs::read_dir(root_path) {
        Ok(it) => it,
        Err(e) if e.kind() == ErrorKind::PermissionDenied => {
            return Err(FsPulseError::Error(format!(
                "Reconcile: root '{}' is unreadable (permission denied)",
                root_path.display()
            )));
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(FsPulseError::Error(format!(
                "Reconcile: root '{}' not found",
                root_path.display()
            )));
        }
        Err(e) => return Err(FsPulseError::from(e)),
    };

    let mut walk = Walk {
        conn,
        root_id,
        root_path,
        allow_empty_root,
        ignore: IgnoreSet::load(conn, root_id)?,
        interrupt,
        on_hint,
        on_progress,
        stats: WalkStats::default(),
    };
    walk.directory(root_path, entries, DirKey::Root)?;
    Ok(walk.stats)
}

/// How a directory being walked relates to the database.
#[derive(Clone, Copy)]
enum DirKey {
    /// The root directory: its children have no parent item.
    Root,
    /// A directory the database has an item for (alive or tombstoned).
    Known(i64),
    /// A directory the database has never seen. Everything in it is new,
    /// so there is nothing to load or compare.
    New,
}

/// A child of a directory as the database records it: the item's latest
/// version.
struct RecordedChild {
    item_id: i64,
    is_deleted: bool,
    access: Access,
    mod_date: Option<i64>,
    size: Option<i64>,
}

struct Walk<'a> {
    conn: &'a Connection,
    root_id: i64,
    root_path: &'a Path,
    allow_empty_root: bool,
    ignore: IgnoreSet,
    interrupt: &'a AtomicBool,
    on_hint: &'a mut dyn FnMut(PathBuf, Difference) -> Result<(), FsPulseError>,
    on_progress: &'a mut dyn FnMut(&WalkStats),
    stats: WalkStats,
}

impl Walk<'_> {
    fn hint(&mut self, path: PathBuf, difference: Difference) -> Result<(), FsPulseError> {
        self.stats.hints += 1;
        (self.on_hint)(path, difference)
    }

    /// How many items applying a `Missing` hint for this child would
    /// mark deleted: the child, plus everything alive under a folder.
    fn alive_items_at(&self, item_id: i64, item_type: ItemType) -> Result<u64, FsPulseError> {
        if item_type != ItemType::Directory {
            return Ok(1);
        }
        let descendants: i64 = self
            .conn
            .prepare_cached(ALIVE_DESCENDANTS_SQL)?
            .query_row(params![self.root_id, item_id], |row| row.get(0))?;
        Ok(1 + descendants as u64)
    }

    fn check_interrupt(&self) -> Result<(), FsPulseError> {
        if self.interrupt.load(Ordering::Acquire) {
            Err(FsPulseError::TaskInterrupted)
        } else {
            Ok(())
        }
    }

    /// Children of a directory as recorded in the database, keyed by
    /// name and type. Uses idx_items_parent_name.
    fn recorded_children(
        &self,
        key: DirKey,
    ) -> Result<HashMap<(String, ItemType), RecordedChild>, FsPulseError> {
        let parent_item_id = match key {
            DirKey::Root => None,
            DirKey::Known(item_id) => Some(item_id),
            DirKey::New => return Ok(HashMap::new()),
        };

        let mut stmt = self.conn.prepare_cached(RECORDED_CHILDREN_SQL)?;
        let mut rows = stmt.query(params![self.root_id, parent_item_id])?;

        let mut children = HashMap::new();
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            let item_type = ItemType::from_i64(row.get(2)?);
            children.insert(
                (name, item_type),
                RecordedChild {
                    item_id: row.get(0)?,
                    is_deleted: row.get(3)?,
                    access: Access::from_i64(row.get(4)?),
                    mod_date: row.get(5)?,
                    size: row.get(6)?,
                },
            );
        }
        Ok(children)
    }

    /// Compare one directory against the database, then recurse into its
    /// readable subdirectories.
    fn directory(
        &mut self,
        dir_path: &Path,
        entries: fs::ReadDir,
        key: DirKey,
    ) -> Result<(), FsPulseError> {
        self.check_interrupt()?;

        let mut recorded = self.recorded_children(key)?;
        // Names already reported in this directory, so a type change
        // (same name, recorded with another type) is reported once.
        let mut hinted_names: HashSet<String> = HashSet::new();
        let mut entries_on_disk = 0u64;
        let mut listing_complete = true;

        for entry in entries {
            // An unexpected I/O error on one entry does not stop the
            // walk. The entry is skipped and the directory is marked
            // incomplete: absence from an incomplete listing proves
            // nothing, so nothing in this directory is reported as gone.
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("reconcile: error listing '{}': {e}", dir_path.display());
                    listing_complete = false;
                    continue;
                }
            };
            let entry_path = entry.path();

            // An ignored entry is treated as absent: it is not examined
            // or descended into. If the database still records it as
            // alive, it is left in `recorded`, reported below with the
            // other missing names, and the applier marks it gone.
            if self.ignore.is_ignored(self.root_path, &entry_path) {
                entries_on_disk += 1;
                continue;
            }

            let metadata = match fs::symlink_metadata(&entry_path) {
                Ok(m) => Some(m),
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) if e.kind() == ErrorKind::PermissionDenied => None,
                Err(e) => {
                    log::warn!("reconcile: cannot stat '{}': {e}", entry_path.display());
                    listing_complete = false;
                    continue;
                }
            };
            entries_on_disk += 1;

            // For directories, open read_dir up front so the Access value
            // compared against the database reflects whether the
            // directory can be listed. The iterator is reused to recurse.
            let dir_iter: Option<Result<fs::ReadDir, ()>> = match metadata.as_ref() {
                Some(m) if m.is_dir() => match fs::read_dir(&entry_path) {
                    Ok(it) => Some(Ok(it)),
                    Err(e) if e.kind() == ErrorKind::PermissionDenied => Some(Err(())),
                    Err(e) if e.kind() == ErrorKind::NotFound => continue,
                    Err(e) => {
                        log::warn!("reconcile: cannot list '{}': {e}", entry_path.display());
                        listing_complete = false;
                        continue;
                    }
                },
                _ => None,
            };

            let access = if metadata.is_none() {
                Access::MetaError
            } else if matches!(dir_iter, Some(Err(()))) {
                Access::ReadError
            } else {
                Access::Ok
            };

            let observed =
                ObservedItem::new(&entry_path, metadata.as_ref(), access, VersionSource::Reconcile)?;
            self.stats.entries += 1;

            let recorded_child = recorded.remove(&(observed.item_name.clone(), observed.item_type));
            let differs = match &recorded_child {
                None => true,
                Some(child) if child.is_deleted => true,
                Some(child) => observed.differs_from(child.access, child.mod_date, child.size),
            };
            if differs {
                hinted_names.insert(observed.item_name.clone());
                self.hint(entry_path.clone(), Difference::Changed)?;
            }

            if let Some(Ok(child_entries)) = dir_iter {
                let child_key = match recorded_child {
                    Some(child) => DirKey::Known(child.item_id),
                    None => DirKey::New,
                };
                self.directory(&entry_path, child_entries, child_key)?;
            }
        }

        // Whatever is left in `recorded` was not found on disk.
        let missing: Vec<(String, ItemType, i64)> = recorded
            .into_iter()
            .filter(|(_, child)| !child.is_deleted)
            .map(|((name, item_type), child)| (name, item_type, child.item_id))
            .collect();

        if matches!(key, DirKey::Root)
            && entries_on_disk == 0
            && !missing.is_empty()
            && !self.allow_empty_root
        {
            return Err(FsPulseError::RootEmpty(format!(
                "'{}' is empty on disk but {} items are recorded directly in it. \
                 The volume may be unmounted; nothing was marked deleted.",
                dir_path.display(),
                missing.len()
            )));
        }

        if listing_complete {
            for (name, item_type, item_id) in missing {
                // A name already reported is a type change: the entry on
                // disk replaces the recorded one when it is applied.
                if hinted_names.insert(name.clone()) {
                    self.stats.missing_items += self.alive_items_at(item_id, item_type)?;
                    self.hint(dir_path.join(name), Difference::Missing)?;
                }
            }
        } else {
            self.stats.incomplete_dirs += 1;
        }

        self.stats.dirs += 1;
        (self.on_progress)(&self.stats);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::applier::{self, Hint};
    use crate::item_ops::test_support::{count, test_db_with_root};
    use crate::item_ops::{BatchStats, BUCKET_SECS};
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    const T0: i64 = 1_700_000_000 - (1_700_000_000 % BUCKET_SECS) + 600;

    struct Fixture {
        dir: TempDir,
        root: PathBuf,
        conn: Connection,
        root_id: i64,
    }

    impl Fixture {
        fn new() -> Fixture {
            let dir = TempDir::new().unwrap();
            // Canonicalize: on macOS the temp dir is reached through a
            // symlink, and paths must match what read_dir returns.
            let root = dir.path().canonicalize().unwrap();
            let (conn, root_id) = test_db_with_root(root.to_str().unwrap());
            Fixture {
                dir,
                root,
                conn,
                root_id,
            }
        }

        fn write(&self, rel: &str, contents: &str) {
            let path = self.root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            File::create(path).unwrap().write_all(contents.as_bytes()).unwrap();
        }

        fn differences(&self) -> Vec<PathBuf> {
            let mut hints = Vec::new();
            find_differences(
                &self.conn,
                self.root_id,
                &self.root,
                false,
                &AtomicBool::new(false),
                &mut |path, _| {
                    hints.push(path);
                    Ok(())
                },
                &mut |_| {},
            )
            .unwrap();
            hints
        }

        /// One full reconcile: find differences, then apply them.
        fn reconcile(&self, now: i64) -> (Vec<PathBuf>, BatchStats) {
            let paths = self.differences();
            let hints: Vec<Hint> = paths
                .iter()
                .map(|path| Hint {
                    root_id: self.root_id,
                    path: path.clone(),
                    source: VersionSource::Reconcile,
                    confirmed: false,
                })
                .collect();
            let stats = applier::apply_hints_at(&self.conn, &hints, now).unwrap();
            (paths, stats)
        }

        fn rel(&self, paths: &[PathBuf]) -> Vec<String> {
            let mut rel: Vec<String> = paths
                .iter()
                .map(|p| p.strip_prefix(&self.root).unwrap().to_string_lossy().into_owned())
                .collect();
            rel.sort();
            rel
        }

        fn alive(&self) -> i64 {
            count(
                &self.conn,
                "SELECT COUNT(*) FROM items i JOIN item_versions iv ON iv.item_id = i.item_id
                 WHERE iv.item_version = (SELECT MAX(item_version) FROM item_versions
                                          WHERE item_id = i.item_id)
                   AND iv.is_deleted = 0",
            )
        }
    }

    fn sample_tree(fx: &Fixture) {
        fx.write("a/one.txt", "1");
        fx.write("a/b/two.txt", "22");
        fx.write("a/b/three.txt", "333");
        fx.write("top.txt", "top");
    }

    #[test]
    fn first_walk_reports_everything_and_second_reports_nothing() {
        let fx = Fixture::new();
        sample_tree(&fx);

        let (paths, stats) = fx.reconcile(T0);
        assert_eq!(
            fx.rel(&paths),
            ["a", "a/b", "a/b/three.txt", "a/b/two.txt", "a/one.txt", "top.txt"]
        );
        assert_eq!(stats.added, 6);
        assert_eq!(fx.alive(), 6);

        // Nothing changed on disk: no hints, and so no writes.
        let before = fx.conn.total_changes();
        let (paths, stats) = fx.reconcile(T0 + 10 * BUCKET_SECS);
        assert!(paths.is_empty(), "unexpected hints: {paths:?}");
        assert_eq!(stats, BatchStats::default());
        assert_eq!(fx.conn.total_changes(), before);
    }

    #[test]
    fn modified_file_is_the_only_hint() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.reconcile(T0);

        fx.write("a/b/two.txt", "a longer body");
        let (paths, stats) = fx.reconcile(T0 + BUCKET_SECS);

        assert_eq!(fx.rel(&paths), ["a/b/two.txt"]);
        assert_eq!((stats.modified, stats.added, stats.deleted), (1, 0, 0));
    }

    #[test]
    fn added_and_deleted_files_are_found() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.reconcile(T0);

        fx.write("a/new.txt", "new");
        fs::remove_file(fx.root.join("top.txt")).unwrap();
        let (paths, stats) = fx.reconcile(T0 + BUCKET_SECS);

        assert_eq!(fx.rel(&paths), ["a/new.txt", "top.txt"]);
        assert_eq!((stats.added, stats.deleted), (1, 1));
        assert_eq!(fx.alive(), 6);
    }

    #[test]
    fn deleted_folder_is_one_hint_that_removes_its_subtree() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.reconcile(T0);

        fs::remove_dir_all(fx.root.join("a")).unwrap();
        let (paths, stats) = fx.reconcile(T0 + BUCKET_SECS);

        // The walk never descends into a folder that is gone, so it is
        // reported once; the applier tombstones everything under it.
        assert_eq!(fx.rel(&paths), ["a"]);
        assert_eq!(stats.deleted, 5);
        assert_eq!(fx.alive(), 1);

        let (paths, _) = fx.reconcile(T0 + 2 * BUCKET_SECS);
        assert!(paths.is_empty());
    }

    #[test]
    fn file_replaced_by_folder_is_reported_once() {
        let fx = Fixture::new();
        fx.write("thing", "a file");
        fx.reconcile(T0);

        fs::remove_file(fx.root.join("thing")).unwrap();
        fx.write("thing/inside.txt", "now a folder");
        let (paths, stats) = fx.reconcile(T0 + BUCKET_SECS);

        assert_eq!(fx.rel(&paths), ["thing", "thing/inside.txt"]);
        assert_eq!((stats.added, stats.deleted), (2, 1));

        let (paths, _) = fx.reconcile(T0 + 2 * BUCKET_SECS);
        assert!(paths.is_empty());
    }

    #[test]
    fn restored_folder_is_rediscovered() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.reconcile(T0);
        fs::remove_dir_all(fx.root.join("a")).unwrap();
        fx.reconcile(T0 + BUCKET_SECS);

        fx.write("a/one.txt", "1");
        let (paths, stats) = fx.reconcile(T0 + 2 * BUCKET_SECS);

        assert_eq!(fx.rel(&paths), ["a", "a/one.txt"]);
        assert_eq!(stats.added, 2);
        assert_eq!(fx.alive(), 3);
    }

    #[test]
    fn empty_root_with_recorded_items_is_refused() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.reconcile(T0);

        fs::remove_dir_all(fx.root.join("a")).unwrap();
        fs::remove_file(fx.root.join("top.txt")).unwrap();

        let mut hints = Vec::new();
        let result = find_differences(
            &fx.conn,
            fx.root_id,
            &fx.root,
            false,
            &AtomicBool::new(false),
            &mut |path, _| {
                hints.push(path);
                Ok(())
            },
            &mut |_| {},
        );

        assert!(result.is_err());
        assert!(hints.is_empty());
        assert_eq!(fx.alive(), 6);
    }

    fn set_ignore(fx: &Fixture, json: Option<&str>) {
        fx.conn
            .execute("UPDATE roots SET ignore_patterns = ?1", [json])
            .unwrap();
    }

    #[test]
    fn ignored_entries_are_never_reported_or_recorded() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.write("a/node_modules/pkg/index.js", "js");
        fx.write("a/b/scratch.tmp", "tmp");
        set_ignore(&fx, Some(r#"["node_modules", "*.tmp"]"#));

        let (paths, stats) = fx.reconcile(T0);
        assert_eq!(
            fx.rel(&paths),
            vec!["a", "a/b", "a/b/three.txt", "a/b/two.txt", "a/one.txt", "top.txt"]
        );
        assert_eq!(stats.added, 6);
        assert_eq!(fx.alive(), 6);

        // Nothing is reported on the next walk either.
        assert!(fx.differences().is_empty());
    }

    #[test]
    fn adding_a_pattern_marks_tracked_items_gone_and_removing_it_restores_them() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.reconcile(T0);
        assert_eq!(fx.alive(), 6);

        // Ignore the folder a/b: it and its two files are marked gone.
        set_ignore(&fx, Some(r#"["a/b"]"#));
        let (paths, stats) = fx.reconcile(T0 + BUCKET_SECS);
        assert_eq!(fx.rel(&paths), vec!["a/b"]);
        assert_eq!(stats.deleted, 3);
        assert_eq!(fx.alive(), 3);
        assert!(fx.differences().is_empty());

        // Remove the pattern: the same items come back to life.
        set_ignore(&fx, None);
        let (_, stats) = fx.reconcile(T0 + 2 * BUCKET_SECS);
        assert_eq!(stats.added, 3);
        assert_eq!(fx.alive(), 6);
        assert_eq!(count(&fx.conn, "SELECT COUNT(*) FROM items"), 6);
    }

    #[test]
    fn an_ignored_entry_still_proves_the_root_is_mounted() {
        let fx = Fixture::new();
        fx.write("keep.txt", "k");
        fx.write("cache/blob", "b");
        fx.reconcile(T0);

        // Only an ignored entry remains on disk. The root is not empty,
        // so the walk runs and the deleted file is recorded as gone.
        set_ignore(&fx, Some(r#"["cache"]"#));
        fs::remove_file(fx.root.join("keep.txt")).unwrap();
        let (_, stats) = fx.reconcile(T0 + BUCKET_SECS);
        assert_eq!(stats.deleted, 3);
        assert_eq!(fx.alive(), 0);
    }

    #[test]
    fn unparseable_patterns_ignore_nothing() {
        let fx = Fixture::new();
        sample_tree(&fx);
        set_ignore(&fx, Some("not json"));
        let (_, stats) = fx.reconcile(T0);
        assert_eq!(stats.added, 6);
    }

    /// Walk and return (path, difference) pairs plus the stats.
    fn walk(fx: &Fixture, allow_empty_root: bool) -> Result<(Vec<(String, Difference)>, WalkStats), FsPulseError> {
        let mut found = Vec::new();
        let stats = find_differences(
            &fx.conn,
            fx.root_id,
            &fx.root,
            allow_empty_root,
            &AtomicBool::new(false),
            &mut |path, difference| {
                let rel = path.strip_prefix(&fx.root).unwrap().to_string_lossy().into_owned();
                found.push((rel, difference));
                Ok(())
            },
            &mut |_| {},
        )?;
        found.sort_by(|a, b| a.0.cmp(&b.0));
        Ok((found, stats))
    }

    #[test]
    fn missing_paths_are_told_apart_and_counted_with_their_subtrees() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.write("a/b/deep/four.txt", "4444");
        fx.reconcile(T0);
        assert_eq!(fx.alive(), 8);

        // Remove folder a/b (5 items: b, two.txt, three.txt, deep,
        // four.txt), remove one file, add one, change one.
        fs::remove_dir_all(fx.root.join("a/b")).unwrap();
        fs::remove_file(fx.root.join("top.txt")).unwrap();
        fx.write("new.txt", "n");
        fx.write("a/one.txt", "longer");

        let (found, stats) = walk(&fx, false).unwrap();
        let of = |kind| -> Vec<&str> {
            found.iter().filter(|(_, d)| *d == kind).map(|(p, _)| p.as_str()).collect()
        };
        assert_eq!(of(Difference::Missing), vec!["a/b", "top.txt"]);
        assert!(of(Difference::Changed).contains(&"new.txt"));
        assert!(of(Difference::Changed).contains(&"a/one.txt"));
        assert_eq!(stats.missing_items, 6);

        // The count is what applying the walk then deletes.
        let (_, applied) = fx.reconcile(T0 + BUCKET_SECS);
        assert_eq!(applied.deleted, 6);
    }

    #[test]
    fn a_type_change_is_not_counted_as_missing() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.reconcile(T0);

        fs::remove_file(fx.root.join("top.txt")).unwrap();
        fs::create_dir(fx.root.join("top.txt")).unwrap();

        let (found, stats) = walk(&fx, false).unwrap();
        assert_eq!(found, vec![("top.txt".to_string(), Difference::Changed)]);
        assert_eq!(stats.missing_items, 0);
    }

    #[test]
    fn an_empty_root_is_walked_only_when_confirmed() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.reconcile(T0);
        fs::remove_dir_all(fx.root.join("a")).unwrap();
        fs::remove_file(fx.root.join("top.txt")).unwrap();

        assert!(matches!(walk(&fx, false), Err(FsPulseError::RootEmpty(_))));

        let (found, stats) = walk(&fx, true).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|(_, d)| *d == Difference::Missing));
        assert_eq!(stats.missing_items, 6);

        // Unconfirmed hints for an empty root are dropped by the applier
        // too; confirmed ones are applied.
        let hints = |confirmed| -> Vec<Hint> {
            found
                .iter()
                .map(|(rel, _)| Hint {
                    root_id: fx.root_id,
                    path: fx.root.join(rel),
                    source: VersionSource::Reconcile,
                    confirmed,
                })
                .collect()
        };
        let later = T0 + BUCKET_SECS;
        let dropped = applier::apply_hints_at(&fx.conn, &hints(false), later).unwrap();
        assert_eq!(dropped.deleted, 0);
        assert_eq!(fx.alive(), 6);

        let applied = applier::apply_hints_at(&fx.conn, &hints(true), later).unwrap();
        assert_eq!(applied.deleted, 6);
        assert_eq!(fx.alive(), 0);
    }

    #[test]
    fn nothing_is_recorded_gone_while_the_root_directory_is_missing() {
        let fx = Fixture::new();
        sample_tree(&fx);
        fx.reconcile(T0);

        // Simulate an unmounted volume: the root directory itself is gone.
        let parked = fx.root.with_extension("parked");
        fs::rename(&fx.root, &parked).unwrap();

        let hints = vec![Hint {
            root_id: fx.root_id,
            path: fx.root.join("a"),
            source: VersionSource::Watch,
            confirmed: false,
        }];
        let stats = applier::apply_hints_at(&fx.conn, &hints, T0 + BUCKET_SECS).unwrap();
        assert_eq!(stats.deleted, 0);
        assert_eq!(fx.alive(), 6);

        fs::rename(&parked, &fx.root).unwrap();
    }

    #[test]
    fn missing_root_is_an_error() {
        let fx = Fixture::new();
        let gone = fx.root.join("does-not-exist");
        let result = find_differences(
            &fx.conn,
            fx.root_id,
            &gone,
            false,
            &AtomicBool::new(false),
            &mut |_, _| Ok(()),
            &mut |_| {},
        );
        assert!(result.is_err());
        drop(fx.dir);
    }

    #[test]
    fn interrupt_stops_the_walk() {
        let fx = Fixture::new();
        sample_tree(&fx);
        let result = find_differences(
            &fx.conn,
            fx.root_id,
            &fx.root,
            false,
            &AtomicBool::new(true),
            &mut |_, _| Ok(()),
            &mut |_| {},
        );
        assert!(matches!(result, Err(FsPulseError::TaskInterrupted)));
    }
}
