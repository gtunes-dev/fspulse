//! Runtime status of each root: how well it is being watched, and
//! whether its reconcile walk is being held back.
//!
//! None of this is stored. It describes what this process currently
//! knows, is rebuilt after a restart, and is read by the API.

use std::collections::HashMap;
use std::sync::RwLock;

use once_cell::sync::Lazy;
use serde::Serialize;

/// How a root is being observed between walks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchState {
    /// Realtime events are being received.
    Watching,
    /// Watching, but the OS reported it could not watch everything
    /// (for example the inotify watch limit). Changes may be missed
    /// until the next walk.
    Degraded,
    /// Watching was requested but could not be set up.
    Unwatched,
    /// The user turned watching off for this root.
    Disabled,
    /// The root directory is not there. The volume is probably
    /// unmounted.
    Offline,
}

impl WatchState {
    /// True when realtime events cover the whole root.
    pub fn is_reliable(self) -> bool {
        self == WatchState::Watching
    }
}

/// Why walks of a root are not being started automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WalkBlock {
    /// The root directory is empty but the database records items in
    /// it: an unmounted volume, or everything really was deleted.
    /// Clears by itself when the directory has content again.
    EmptyRoot,
    /// A walk found that most of the root's items are missing and
    /// refused to record them as deleted. Clears when the user confirms.
    MassDelete,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RootStatus {
    /// `None` until the watcher has looked at the root.
    pub watch: Option<WatchState>,
    /// Detail for a state other than `Watching`, for display.
    pub watch_detail: Option<String>,
    pub walk_block: Option<WalkBlock>,
    /// What the walk found when it was blocked, for display.
    pub walk_block_detail: Option<String>,
}

static STATUS: Lazy<RwLock<HashMap<i64, RootStatus>>> = Lazy::new(|| RwLock::new(HashMap::new()));

pub fn get(root_id: i64) -> RootStatus {
    STATUS
        .read()
        .unwrap()
        .get(&root_id)
        .cloned()
        .unwrap_or_default()
}

pub fn set_watch(root_id: i64, state: WatchState, detail: Option<String>) {
    let mut all = STATUS.write().unwrap();
    let status = all.entry(root_id).or_default();
    status.watch = Some(state);
    status.watch_detail = detail;
}

pub fn set_walk_block(root_id: i64, block: WalkBlock, detail: String) {
    let mut all = STATUS.write().unwrap();
    let status = all.entry(root_id).or_default();
    status.walk_block = Some(block);
    status.walk_block_detail = Some(detail);
}

pub fn clear_walk_block(root_id: i64) {
    if let Some(status) = STATUS.write().unwrap().get_mut(&root_id) {
        status.walk_block = None;
        status.walk_block_detail = None;
    }
}

/// Drop the status of a root that no longer exists.
pub fn forget(root_id: i64) {
    STATUS.write().unwrap().remove(&root_id);
}
