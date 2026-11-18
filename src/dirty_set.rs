//! The dirty set: paths the watcher has heard about and not yet handed
//! to the applier.
//!
//! This is the per-path debounce. A path is released once it has been
//! quiet for a settle delay, or once it has waited a maximum time, so a
//! file that never stops changing is still observed. A path that turns
//! dirty again right after being released is "hot": each time that
//! happens its delays double, up to five minutes. A path that stays
//! quiet for a while after a release starts over with the short delays.
//!
//! The set holds only paths, never file state. What a path looks like is
//! decided by the applier's stat at the time it is released.
//!
//! Time is passed in by the caller so tests can drive it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A path is released after this long with no further events.
pub const SETTLE: Duration = Duration::from_secs(2);

/// A path is released after this long even if events keep arriving.
pub const MAX_WAIT: Duration = Duration::from_secs(30);

/// Upper bound for both delays once a path has backed off.
pub const MAX_DELAY: Duration = Duration::from_secs(300);

/// A path that turns dirty again within this long after being released
/// is hot, and its delays double.
pub const HOT_GAP: Duration = Duration::from_secs(10);

/// Highest backoff level. `SETTLE << 8` is past `MAX_DELAY`.
const MAX_LEVEL: u32 = 8;

/// A dirty path: the root it belongs to and its absolute path.
pub type DirtyKey = (i64, PathBuf);

struct Pending {
    first_event: Instant,
    last_event: Instant,
    /// Backoff level in effect for this wait.
    level: u32,
}

impl Pending {
    fn due_at(&self) -> Instant {
        let quiet = self.last_event + settle_for(self.level);
        let capped = self.first_event + max_wait_for(self.level);
        quiet.min(capped)
    }
}

/// The level and time of a path's most recent release.
struct Released {
    level: u32,
    at: Instant,
}

fn settle_for(level: u32) -> Duration {
    SETTLE.saturating_mul(1 << level.min(MAX_LEVEL)).min(MAX_DELAY)
}

fn max_wait_for(level: u32) -> Duration {
    MAX_WAIT.saturating_mul(1 << level.min(MAX_LEVEL)).min(MAX_DELAY)
}

#[derive(Default)]
pub struct DirtySet {
    pending: HashMap<DirtyKey, Pending>,
    released: HashMap<DirtyKey, Released>,
}

impl DirtySet {
    pub fn new() -> DirtySet {
        DirtySet::default()
    }

    /// Number of paths waiting to be released.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Record an event for a path.
    pub fn mark(&mut self, key: DirtyKey, now: Instant) {
        if let Some(pending) = self.pending.get_mut(&key) {
            pending.last_event = now;
            return;
        }

        let level = match self.released.remove(&key) {
            Some(last) if now.saturating_duration_since(last.at) <= HOT_GAP => {
                (last.level + 1).min(MAX_LEVEL)
            }
            _ => 0,
        };
        self.pending.insert(
            key,
            Pending {
                first_event: now,
                last_event: now,
                level,
            },
        );
    }

    /// Remove and return every path that is due at `now`.
    pub fn take_due(&mut self, now: Instant) -> Vec<DirtyKey> {
        // Forget releases too old to make a path hot.
        self.released
            .retain(|_, last| now.saturating_duration_since(last.at) <= HOT_GAP);

        let due: Vec<DirtyKey> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.due_at() <= now)
            .map(|(key, _)| key.clone())
            .collect();

        for key in &due {
            if let Some(pending) = self.pending.remove(key) {
                self.released.insert(
                    key.clone(),
                    Released {
                        level: pending.level,
                        at: now,
                    },
                );
            }
        }
        due
    }

    /// Drop every waiting path of one root without releasing it. Used
    /// when a root is about to be walked in full, or is no longer
    /// watched. Returns how many were dropped.
    pub fn drop_root(&mut self, root_id: i64) -> usize {
        let before = self.pending.len();
        self.pending.retain(|(id, _), _| *id != root_id);
        self.released.retain(|(id, _), _| *id != root_id);
        before - self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> DirtyKey {
        (1, PathBuf::from(name))
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn millis(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn released_after_the_settle_delay() {
        let t0 = Instant::now();
        let mut set = DirtySet::new();
        set.mark(key("a"), t0);

        assert!(set.take_due(t0 + millis(1900)).is_empty());
        assert_eq!(set.take_due(t0 + secs(2)), vec![key("a")]);
        assert_eq!(set.len(), 0);
        assert!(set.take_due(t0 + secs(60)).is_empty());
    }

    #[test]
    fn events_restart_the_settle_delay() {
        let t0 = Instant::now();
        let mut set = DirtySet::new();
        set.mark(key("a"), t0);
        set.mark(key("a"), t0 + secs(1));
        set.mark(key("a"), t0 + millis(2500));

        assert_eq!(set.len(), 1);
        assert!(set.take_due(t0 + secs(4)).is_empty());
        assert_eq!(set.take_due(t0 + millis(4500)), vec![key("a")]);
    }

    #[test]
    fn a_path_that_never_goes_quiet_is_released_at_the_maximum_wait() {
        let t0 = Instant::now();
        let mut set = DirtySet::new();
        let mut released_at = None;
        for tick in 0..400 {
            let now = t0 + millis(tick * 100);
            set.mark(key("busy"), now);
            if !set.take_due(now).is_empty() {
                released_at = Some(now - t0);
                break;
            }
        }
        assert_eq!(released_at, Some(MAX_WAIT));
    }

    #[test]
    fn a_continuously_written_path_backs_off_to_the_maximum_delay() {
        let t0 = Instant::now();
        let mut set = DirtySet::new();
        let mut releases = Vec::new();
        // One event per second for an hour.
        for tick in 0..3600 {
            let now = t0 + secs(tick);
            set.mark(key("log"), now);
            if !set.take_due(now).is_empty() {
                releases.push(tick);
            }
        }

        // 30s, then 60s, 120s, 240s, then every 300s.
        assert_eq!(&releases[..5], &[30, 91, 212, 453, 754]);
        let gaps: Vec<u64> = releases.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps[4..].iter().all(|gap| *gap == 301), "{gaps:?}");
        assert!(releases.len() <= 15, "{} releases in an hour", releases.len());
    }

    #[test]
    fn a_path_that_goes_quiet_starts_over() {
        let t0 = Instant::now();
        let mut set = DirtySet::new();

        // Make the path hot: dirty again immediately after each release.
        set.mark(key("a"), t0);
        assert_eq!(set.take_due(t0 + secs(2)).len(), 1);
        set.mark(key("a"), t0 + secs(3));
        // Level 1: settle is 4s.
        assert!(set.take_due(t0 + secs(6)).is_empty());
        assert_eq!(set.take_due(t0 + secs(7)).len(), 1);

        // Quiet for longer than HOT_GAP, then one more event.
        let later = t0 + secs(7) + HOT_GAP + secs(1);
        set.mark(key("a"), later);
        assert_eq!(set.take_due(later + SETTLE).len(), 1);
    }

    #[test]
    fn paths_are_independent_and_roots_can_be_dropped() {
        let t0 = Instant::now();
        let mut set = DirtySet::new();
        set.mark((1, PathBuf::from("a")), t0);
        set.mark((1, PathBuf::from("b")), t0 + secs(1));
        set.mark((2, PathBuf::from("a")), t0);

        let mut due = set.take_due(t0 + secs(2));
        due.sort();
        assert_eq!(due, vec![(1, PathBuf::from("a")), (2, PathBuf::from("a"))]);

        set.mark((2, PathBuf::from("c")), t0 + secs(2));
        assert_eq!(set.drop_root(1), 1);
        assert_eq!(set.len(), 1);
        assert_eq!(set.take_due(t0 + secs(10)), vec![(2, PathBuf::from("c"))]);
    }
}
