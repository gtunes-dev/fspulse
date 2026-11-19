//! The watcher: realtime filesystem events, turned into hints.
//!
//! The watcher never looks at what an event says happened, and never
//! writes to the database. Every event is reduced to "this path may have
//! changed". Paths wait in the [`DirtySet`] until they settle, then go
//! to the applier, which stats each one and records what is on disk.
//!
//! What the watcher cannot see, the reconcile walk catches. When the
//! operating system reports that events were dropped, or more paths are
//! dirty than the watcher is willing to hold, the watcher gives up on
//! the affected root's pending paths and queues a full walk instead.
//!
//! The watcher also decides when walks happen. Every root is walked
//! once when the watcher first sees it (at startup, when it is added,
//! and when its volume comes back), and again whenever it has gone too
//! long without a completed walk: a day for a root that is reliably
//! watched, a few hours for one that is not.
//!
//! The list of roots, their `watch_enabled` setting and their ignore
//! patterns are re-read from the database periodically, so changes to
//! roots take effect without restarting the watcher. Each root's watch
//! state is published through `root_status`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, RecvTimeoutError};
use log::{error, info, warn};
use notify::{Event, EventKind, RecursiveMode, Watcher};

use crate::applier::{Applier, Hint};
use crate::db::Database;
use crate::dirty_set::{DirtyKey, DirtySet};
use crate::error::FsPulseError;
use crate::ignore::IgnoreSet;
use crate::item_identity::VersionSource;
use crate::root_status::{self, WalkBlock, WatchState};
use crate::task::{CheckpointSettings, TaskStatus, TaskType};
use crate::task_manager::TaskManager;
use crate::utils::Utils;

/// How long the event loop waits for an event before checking its
/// timers and the interrupt flag.
const POLL: Duration = Duration::from_millis(500);

/// How often settled paths are handed to the applier. Together with the
/// applier's batching this bounds how often the database is written,
/// whatever the event rate.
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);

/// How often the roots table is re-read.
const ROOTS_REFRESH: Duration = Duration::from_secs(30);

/// How long a reliably watched root may go without a completed walk.
const WALK_INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// The same for a root that is not reliably watched (watching turned
/// off, failed, or degraded).
const WALK_INTERVAL_UNRELIABLE: Duration = Duration::from_secs(4 * 3600);

/// How long to wait before queueing another walk of a root whose last
/// one did not finish.
const WALK_RETRY: Duration = Duration::from_secs(15 * 60);

/// Most paths the dirty set may hold. Past this, a full walk of the
/// root is cheaper and safer than tracking paths one by one.
const MAX_DIRTY_PATHS: usize = 100_000;

/// A root the watcher is responsible for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchedRoot {
    root_id: i64,
    path: PathBuf,
    ignore: IgnoreSet,
}

/// What one filesystem event means for the watcher.
#[derive(Debug, Default, PartialEq, Eq)]
struct Routed {
    /// Paths to mark dirty, one entry per root that contains the path.
    dirty: Vec<DirtyKey>,
    /// Roots whose events were dropped and that need a full walk.
    rescan: Vec<i64>,
}

/// Decide what an event means: which paths are dirty in which roots, or
/// which roots need a full walk.
///
/// A path under nested roots is dirty in each of them. The root
/// directory itself is not an item and is never marked. Ignored paths
/// are dropped here, before they cost anything.
fn route(roots: &[WatchedRoot], event: &Event) -> Routed {
    let mut routed = Routed::default();

    if event.need_rescan() {
        // The OS dropped events. With no paths, every root is affected;
        // otherwise the roots that contain, or sit under, a named path.
        routed.rescan = roots
            .iter()
            .filter(|root| {
                event.paths.is_empty()
                    || event
                        .paths
                        .iter()
                        .any(|path| path.starts_with(&root.path) || root.path.starts_with(path))
            })
            .map(|root| root.root_id)
            .collect();
        return routed;
    }

    // Opening or reading a file changes nothing we record.
    if matches!(event.kind, EventKind::Access(_)) {
        return routed;
    }

    for path in &event.paths {
        for root in roots {
            if path != &root.path
                && path.starts_with(&root.path)
                && !root.ignore.is_ignored(&root.path, path)
            {
                routed.dirty.push((root.root_id, path.clone()));
            }
        }
    }
    routed
}

/// Of the given roots, the directories to register with the OS: a root
/// nested under another is already covered by the outer root's
/// recursive watch.
fn watch_paths(roots: &[WatchedRoot]) -> HashSet<PathBuf> {
    roots
        .iter()
        .filter(|candidate| {
            !roots.iter().any(|other| {
                other.path != candidate.path && candidate.path.starts_with(&other.path)
            })
        })
        .map(|root| root.path.clone())
        .collect()
}

/// A row of `roots`, as the watcher needs it.
#[derive(Debug, Clone)]
struct RootRow {
    root_id: i64,
    path: PathBuf,
    watch_enabled: bool,
    ignore: IgnoreSet,
    /// When the last walk finished. `None` if none ever has, or the
    /// most recent one did not finish.
    last_walk_completed_at: Option<i64>,
    /// A walk of this root is waiting to run or running.
    walk_queued: bool,
}

/// Read every root.
fn load_roots() -> Result<Vec<RootRow>, FsPulseError> {
    let conn = Database::get_connection()?;
    let mut stmt = conn.prepare(
        "SELECT r.root_id, r.root_path, r.watch_enabled, r.ignore_patterns,
                r.last_checkpoint_completed_at,
                EXISTS (SELECT 1 FROM tasks t
                         WHERE t.task_type = ?1 AND t.root_id = r.root_id
                           AND t.status IN (?2, ?3))
           FROM roots r",
    )?;
    let rows = stmt
        .query_map(
            rusqlite::params![
                TaskType::Checkpoint.as_i64(),
                TaskStatus::Pending.as_i64(),
                TaskStatus::Running.as_i64(),
            ],
            |row| {
                let root_id: i64 = row.get(0)?;
                let patterns: Option<String> = row.get(3)?;
                Ok(RootRow {
                    root_id,
                    path: PathBuf::from(row.get::<_, String>(1)?),
                    watch_enabled: row.get(2)?,
                    ignore: IgnoreSet::from_json_lenient(patterns.as_deref(), root_id),
                    last_walk_completed_at: row.get(4)?,
                    walk_queued: row.get(5)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// How long a root may go without a completed walk. A root that is
/// reliably watched needs a walk only as a safety net; one that is not
/// depends on walks to notice changes at all.
fn walk_interval(state: WatchState) -> Duration {
    if state.is_reliable() {
        WALK_INTERVAL
    } else {
        WALK_INTERVAL_UNRELIABLE
    }
}

/// Decide whether a periodic walk of a root should be queued now.
/// `since_request` is how long ago this process last queued one.
fn walk_is_due(
    row: &RootRow,
    state: WatchState,
    since_request: Option<Duration>,
    now_secs: i64,
) -> bool {
    if row.walk_queued {
        return false;
    }
    // A walk that failed leaves no completion time. Do not retry it on
    // every refresh.
    if since_request.is_some_and(|elapsed| elapsed < WALK_RETRY) {
        return false;
    }
    match row.last_walk_completed_at {
        None => true,
        Some(completed_at) => {
            now_secs.saturating_sub(completed_at) >= walk_interval(state).as_secs() as i64
        }
    }
}

fn directory_has_content(path: &std::path::Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

struct WatchLoop {
    watcher: notify::RecommendedWatcher,
    /// Roots that are enabled and present: the ones events are routed to.
    roots: Vec<WatchedRoot>,
    /// Every root seen at the last refresh.
    known: HashSet<i64>,
    /// Directories currently registered with the OS.
    watching: HashSet<PathBuf>,
    /// Registered directories the OS has said it cannot fully watch.
    degraded: HashSet<PathBuf>,
    /// Directories that could not be registered, with the error last
    /// logged, so a persistent failure is logged once.
    failed: HashMap<PathBuf, String>,
    dirty: DirtySet,
    /// Roots that need a full walk which has not been queued yet.
    needs_walk: HashSet<i64>,
    /// Roots this process has asked to walk since they were last seen
    /// present. A root missing from this set gets a walk as soon as it
    /// is seen: at startup, when added, and when its volume returns.
    walk_requested_once: HashSet<i64>,
    /// When this process last queued a walk of each root.
    last_walk_request: HashMap<i64, Instant>,
}

impl WatchLoop {
    /// Re-read the roots table, bring the OS registrations in line with
    /// it, publish each root's watch state, and decide which roots are
    /// owed a walk.
    fn refresh_roots(&mut self, now: Instant) {
        let rows = match load_roots() {
            Ok(rows) => rows,
            Err(e) => {
                error!("Watcher: could not read roots: {e}");
                return;
            }
        };

        let present: HashSet<i64> = rows
            .iter()
            .filter(|row| row.path.is_dir())
            .map(|row| row.root_id)
            .collect();
        let roots: Vec<WatchedRoot> = rows
            .iter()
            .filter(|row| row.watch_enabled && present.contains(&row.root_id))
            .map(|row| WatchedRoot {
                root_id: row.root_id,
                path: row.path.clone(),
                ignore: row.ignore.clone(),
            })
            .collect();

        self.register(&watch_paths(&roots));

        // Paths waiting for a root that is no longer watched would be
        // stale by the time they were applied; a deleted root's would be
        // applied to nothing.
        let watched: HashSet<i64> = roots.iter().map(|root| root.root_id).collect();
        for root in &self.roots {
            if !watched.contains(&root.root_id) {
                self.dirty.drop_root(root.root_id);
            }
        }
        let known: HashSet<i64> = rows.iter().map(|row| row.root_id).collect();
        for root_id in self.known.difference(&known) {
            self.needs_walk.remove(root_id);
            self.walk_requested_once.remove(root_id);
            self.last_walk_request.remove(root_id);
            root_status::forget(*root_id);
        }
        self.roots = roots;
        self.known = known;

        let now_secs = Utils::now_secs();
        for row in &rows {
            let (state, detail) = self.watch_state(row, present.contains(&row.root_id));
            root_status::set_watch(row.root_id, state, detail);

            if state == WatchState::Offline {
                // Nothing to walk. Walk it as soon as it is back.
                self.needs_walk.remove(&row.root_id);
                self.walk_requested_once.remove(&row.root_id);
                continue;
            }

            match root_status::get(row.root_id).walk_block {
                // Only the user can release this one.
                Some(WalkBlock::MassDelete) => continue,
                Some(WalkBlock::EmptyRoot) => {
                    if !directory_has_content(&row.path) {
                        continue;
                    }
                    // The volume is back.
                    root_status::clear_walk_block(row.root_id);
                    self.last_walk_request.remove(&row.root_id);
                }
                None => {}
            }

            let first_sight = self.walk_requested_once.insert(row.root_id);
            let since_request = self
                .last_walk_request
                .get(&row.root_id)
                .map(|at| now.saturating_duration_since(*at));
            if first_sight || walk_is_due(row, state, since_request, now_secs) {
                self.needs_walk.insert(row.root_id);
            }
        }
    }

    /// Make the set of directories registered with the OS equal `wanted`.
    fn register(&mut self, wanted: &HashSet<PathBuf>) {
        for path in self.watching.difference(wanted).cloned().collect::<Vec<_>>() {
            if let Err(e) = self.watcher.unwatch(&path) {
                warn!("Watcher: could not stop watching {}: {e}", path.display());
            }
            self.watching.remove(&path);
            self.degraded.remove(&path);
            info!("Watcher: stopped watching {}", path.display());
        }
        self.failed.retain(|path, _| wanted.contains(path));

        for path in wanted.difference(&self.watching).cloned().collect::<Vec<_>>() {
            match self.watcher.watch(&path, RecursiveMode::Recursive) {
                Ok(()) => {
                    info!("Watcher: watching {}", path.display());
                    self.failed.remove(&path);
                    self.watching.insert(path);
                }
                Err(e) => {
                    let message = e.to_string();
                    if self.failed.get(&path) != Some(&message) {
                        error!("Watcher: cannot watch {}: {message}", path.display());
                        self.failed.insert(path, message);
                    }
                }
            }
        }
    }

    /// The watch state of one root, with detail for display.
    fn watch_state(&self, row: &RootRow, present: bool) -> (WatchState, Option<String>) {
        if !present {
            return (
                WatchState::Offline,
                Some("The root directory was not found.".to_string()),
            );
        }
        if !row.watch_enabled {
            return (WatchState::Disabled, None);
        }
        // The registered directory that covers this root: the root
        // itself, or an outer root it is nested in.
        let registered = |set: &HashSet<PathBuf>| set.iter().any(|path| row.path.starts_with(path));
        if registered(&self.degraded) {
            (
                WatchState::Degraded,
                Some("The operating system could not watch every directory.".to_string()),
            )
        } else if registered(&self.watching) {
            (WatchState::Watching, None)
        } else {
            let reason = self
                .failed
                .iter()
                .find(|(path, _)| row.path.starts_with(path))
                .map(|(_, message)| message.clone());
            (WatchState::Unwatched, reason)
        }
    }

    fn handle(&mut self, event: notify::Result<Event>, now: Instant) {
        let event = match event {
            Ok(event) => event,
            Err(e) => {
                error!("Watcher: {e}");
                if matches!(e.kind, notify::ErrorKind::MaxFilesWatch) {
                    // Without paths the error is about everything.
                    let affected: Vec<PathBuf> = self
                        .watching
                        .iter()
                        .filter(|watched| {
                            e.paths.is_empty()
                                || e.paths.iter().any(|path| path.starts_with(watched))
                        })
                        .cloned()
                        .collect();
                    self.degraded.extend(affected);
                }
                return;
            }
        };

        let routed = route(&self.roots, &event);

        for root_id in routed.rescan {
            warn!("Watcher: events were dropped for root {root_id}; a full walk is needed");
            self.abandon_root(root_id);
        }

        for key in routed.dirty {
            if self.dirty.len() >= MAX_DIRTY_PATHS {
                warn!(
                    "Watcher: more than {MAX_DIRTY_PATHS} paths are waiting; \
                     root {} will be walked in full instead",
                    key.0
                );
                self.abandon_root(key.0);
                continue;
            }
            self.dirty.mark(key, now);
        }
    }

    /// Stop tracking a root path by path and have it walked instead.
    fn abandon_root(&mut self, root_id: i64) {
        self.dirty.drop_root(root_id);
        self.needs_walk.insert(root_id);
    }

    /// Hand settled paths to the applier and queue any walks that are
    /// owed.
    fn flush(&mut self, now: Instant) {
        for (root_id, path) in self.dirty.take_due(now) {
            let hint = Hint {
                root_id,
                path,
                source: VersionSource::Watch,
                confirmed: false,
            };
            if let Err(e) = Applier::submit(hint) {
                error!("Watcher: {e}");
                return;
            }
        }

        if self.needs_walk.is_empty() {
            return;
        }
        let conn = match Database::get_connection() {
            Ok(conn) => conn,
            Err(e) => {
                error!("Watcher: could not queue a walk: {e}");
                return;
            }
        };
        // A request that fails (for example while the database is being
        // compacted) stays in the set and is retried at the next flush.
        let settings = CheckpointSettings::default();
        let last_walk_request = &mut self.last_walk_request;
        self.needs_walk.retain(|root_id| {
            match TaskManager::request_checkpoint(&conn, *root_id, &settings) {
                Ok(()) => {
                    info!("Watcher: queued a full walk of root {root_id}");
                    last_walk_request.insert(*root_id, now);
                    false
                }
                Err(e) => {
                    warn!("Watcher: could not queue a walk of root {root_id} yet: {e}");
                    true
                }
            }
        });
    }
}

static REFRESH_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Ask the watcher to re-read the roots table now instead of at its
/// next periodic refresh. Called when a root's settings are saved, so
/// that a removed ignore pattern stops hiding events within a moment,
/// not up to `ROOTS_REFRESH` later.
pub fn request_refresh() {
    REFRESH_REQUESTED.store(true, Ordering::Release);
}

/// Run the watcher. Blocks until the interrupt flag is set.
pub fn watch_roots(interrupt: Arc<AtomicBool>) -> Result<(), FsPulseError> {
    let (tx, rx) = unbounded::<notify::Result<Event>>();
    let watcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })
    .map_err(|e| FsPulseError::Error(format!("Failed to create file watcher: {e}")))?;

    let mut state = WatchLoop {
        watcher,
        roots: Vec::new(),
        known: HashSet::new(),
        watching: HashSet::new(),
        degraded: HashSet::new(),
        failed: HashMap::new(),
        dirty: DirtySet::new(),
        needs_walk: HashSet::new(),
        walk_requested_once: HashSet::new(),
        last_walk_request: HashMap::new(),
    };

    // Watch first, then walk: every root is owed a walk the first time
    // it is seen, and by then its events are already being collected,
    // so nothing that happens during the walk is missed.
    state.refresh_roots(Instant::now());
    info!("Watcher: started, watching {} roots", state.roots.len());

    let mut last_flush = Instant::now();
    let mut last_refresh = Instant::now();

    loop {
        if interrupt.load(Ordering::Relaxed) {
            info!("Watcher: interrupt received, stopping");
            break;
        }

        match rx.recv_timeout(POLL) {
            Ok(event) => {
                let now = Instant::now();
                state.handle(event, now);
                // Take whatever else has already arrived without waiting.
                for event in rx.try_iter() {
                    state.handle(event, now);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                warn!("Watcher: event channel closed, stopping");
                break;
            }
        }

        let now = Instant::now();
        if now.duration_since(last_flush) >= FLUSH_INTERVAL {
            state.flush(now);
            last_flush = now;
        }
        if REFRESH_REQUESTED.swap(false, Ordering::AcqRel)
            || now.duration_since(last_refresh) >= ROOTS_REFRESH
        {
            state.refresh_roots(now);
            last_refresh = now;
        }
    }

    // Paths still waiting are dropped: the walk at next startup finds
    // whatever they would have recorded.
    info!("Watcher: stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::applier;
    use crate::item_ops::test_support::{count, test_db_with_root};
    use crate::item_ops::BUCKET_SECS;
    use notify::event::{AccessKind, CreateKind, Flag, ModifyKind};
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn root(root_id: i64, path: &str, patterns: &[&str]) -> WatchedRoot {
        WatchedRoot {
            root_id,
            path: PathBuf::from(path),
            ignore: IgnoreSet::new(patterns),
        }
    }

    fn modify(path: &str) -> Event {
        Event::new(EventKind::Modify(ModifyKind::Any)).add_path(PathBuf::from(path))
    }

    #[test]
    fn a_path_is_dirty_in_every_root_that_contains_it() {
        let roots = [root(1, "/data", &[]), root(2, "/data/photos", &[]), root(3, "/other", &[])];
        let routed = route(&roots, &modify("/data/photos/a.jpg"));
        assert_eq!(
            routed.dirty,
            vec![
                (1, PathBuf::from("/data/photos/a.jpg")),
                (2, PathBuf::from("/data/photos/a.jpg")),
            ]
        );
        assert!(routed.rescan.is_empty());
    }

    #[test]
    fn the_root_directory_itself_is_not_an_item() {
        let roots = [root(1, "/data", &[]), root(2, "/data/photos", &[])];
        let routed = route(&roots, &modify("/data/photos"));
        // An item of root 1, but the root directory of root 2.
        assert_eq!(routed.dirty, vec![(1, PathBuf::from("/data/photos"))]);
    }

    #[test]
    fn ignored_paths_and_access_events_are_dropped() {
        let roots = [root(1, "/data", &["node_modules", "*.tmp"])];
        assert!(route(&roots, &modify("/data/app/node_modules/x/index.js")).dirty.is_empty());
        assert!(route(&roots, &modify("/data/a.tmp")).dirty.is_empty());
        assert_eq!(route(&roots, &modify("/data/a.txt")).dirty.len(), 1);

        let access = Event::new(EventKind::Access(AccessKind::Any))
            .add_path(PathBuf::from("/data/a.txt"));
        assert_eq!(route(&roots, &access), Routed::default());
    }

    #[test]
    fn a_rename_marks_both_paths() {
        let roots = [root(1, "/data", &[])];
        let rename = Event::new(EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Both)))
            .add_path(PathBuf::from("/data/old.txt"))
            .add_path(PathBuf::from("/data/new.txt"));
        assert_eq!(route(&roots, &rename).dirty.len(), 2);
    }

    #[test]
    fn dropped_events_ask_for_a_walk_instead_of_marking_paths() {
        let roots = [root(1, "/data", &[]), root(2, "/data/photos", &[]), root(3, "/other", &[])];

        let everything = Event::new(EventKind::Other).set_flag(Flag::Rescan);
        assert_eq!(route(&roots, &everything).rescan, vec![1, 2, 3]);

        let one_subtree = Event::new(EventKind::Other)
            .set_flag(Flag::Rescan)
            .add_path(PathBuf::from("/data/photos/2024"));
        let routed = route(&roots, &one_subtree);
        assert_eq!(routed.rescan, vec![1, 2]);
        assert!(routed.dirty.is_empty());

        let above = Event::new(EventKind::Other)
            .set_flag(Flag::Rescan)
            .add_path(PathBuf::from("/data"));
        assert_eq!(route(&roots, &above).rescan, vec![1, 2]);
    }

    #[test]
    fn nested_roots_share_the_outer_watch() {
        let roots = [
            root(1, "/data", &[]),
            root(2, "/data/photos", &[]),
            root(3, "/other", &[]),
            root(4, "/database", &[]),
        ];
        let mut paths: Vec<PathBuf> = watch_paths(&roots).into_iter().collect();
        paths.sort();
        assert_eq!(
            paths,
            vec![PathBuf::from("/data"), PathBuf::from("/database"), PathBuf::from("/other")]
        );
    }

    fn row(last_walk_completed_at: Option<i64>, walk_queued: bool) -> RootRow {
        RootRow {
            root_id: 1,
            path: PathBuf::from("/data"),
            watch_enabled: true,
            ignore: IgnoreSet::default(),
            last_walk_completed_at,
            walk_queued,
        }
    }

    #[test]
    fn a_watched_root_is_walked_daily_and_an_unwatched_one_more_often() {
        let now = 1_700_000_000;
        let hours = |n: i64| Some(now - n * 3600);

        assert!(!walk_is_due(&row(hours(23), false), WatchState::Watching, None, now));
        assert!(walk_is_due(&row(hours(24), false), WatchState::Watching, None, now));

        for state in [WatchState::Disabled, WatchState::Unwatched, WatchState::Degraded] {
            assert!(!walk_is_due(&row(hours(3), false), state, None, now));
            assert!(walk_is_due(&row(hours(4), false), state, None, now));
        }
    }

    #[test]
    fn a_walk_is_not_queued_twice_or_retried_at_once() {
        let now = 1_700_000_000;
        let minutes = |n: u64| Some(Duration::from_secs(n * 60));

        // Never completed: due.
        assert!(walk_is_due(&row(None, false), WatchState::Watching, None, now));
        // One is already waiting or running.
        assert!(!walk_is_due(&row(None, true), WatchState::Watching, None, now));
        // The last one this process queued did not finish; wait.
        assert!(!walk_is_due(&row(None, false), WatchState::Watching, minutes(14), now));
        assert!(walk_is_due(&row(None, false), WatchState::Watching, minutes(15), now));
    }

    /// The phase 4 acceptance case: a file written 10,000 times in an
    /// hour ends up as one version. The dirty set is driven with
    /// simulated time; each release is applied at the matching clock
    /// time.
    #[test]
    fn ten_thousand_writes_in_an_hour_make_one_version() {
        let dir = TempDir::new().unwrap();
        let root_path = dir.path().canonicalize().unwrap();
        let (conn, root_id) = test_db_with_root(root_path.to_str().unwrap());
        let roots = [WatchedRoot {
            root_id,
            path: root_path.clone(),
            ignore: IgnoreSet::default(),
        }];
        let file_path = root_path.join("busy.log");
        let mut file = fs::File::create(&file_path).unwrap();

        let hour_start = 1_700_000_000 - (1_700_000_000 % BUCKET_SECS);
        let t0 = Instant::now();
        let mut dirty = DirtySet::new();
        let mut releases = 0;
        let mut transactions = 0;

        let event = Event::new(EventKind::Create(CreateKind::File)).add_path(file_path.clone());
        const WRITES: u64 = 10_000;
        for write in 0..WRITES {
            // Spread the writes over 59 minutes.
            let elapsed = Duration::from_millis(write * 3_540_000 / WRITES);
            file.write_all(b"x").unwrap();
            file.flush().unwrap();

            for key in route(&roots, &event).dirty {
                dirty.mark(key, t0 + elapsed);
            }

            let hints: Vec<Hint> = dirty
                .take_due(t0 + elapsed)
                .into_iter()
                .map(|(root_id, path)| Hint {
                    root_id,
                    path,
                    source: VersionSource::Watch,
                    confirmed: false,
                })
                .collect();
            if !hints.is_empty() {
                releases += hints.len();
                transactions += 1;
                let now = hour_start + elapsed.as_secs() as i64;
                applier::apply_hints_at(&conn, &hints, now).unwrap();
            }
        }

        assert_eq!(count(&conn, "SELECT COUNT(*) FROM items"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM item_versions"), 1);
        assert_eq!(
            count(&conn, "SELECT change_count FROM item_versions"),
            releases as i64
        );
        assert_eq!(count(&conn, "SELECT source FROM item_versions"), 2);
        // Backoff keeps a constantly written file to a few writes an hour.
        assert!(transactions <= 15, "{transactions} transactions");
    }

    /// A burst of new files settles together and is released together.
    #[test]
    fn a_large_burst_is_released_in_one_flush() {
        let roots = [root(1, "/data", &[".git"])];
        let t0 = Instant::now();
        let mut dirty = DirtySet::new();

        for n in 0..5_000u64 {
            let at = t0 + Duration::from_micros(n * 200);
            for path in [format!("/data/src/file{n}.rs"), format!("/data/.git/objects/{n}")] {
                let event = Event::new(EventKind::Create(CreateKind::File)).add_path(path.into());
                for key in route(&roots, &event).dirty {
                    dirty.mark(key, at);
                }
            }
        }

        assert_eq!(dirty.len(), 5_000);
        assert!(dirty.take_due(t0 + Duration::from_millis(1900)).is_empty());
        assert_eq!(dirty.take_due(t0 + FLUSH_INTERVAL).len(), 5_000);
        assert_eq!(dirty.len(), 0);
    }
}
