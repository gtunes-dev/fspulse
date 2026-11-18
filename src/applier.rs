//! The applier: turns "this path may have changed" hints into database
//! writes.
//!
//! Neither the reconcile walk nor the watcher writes what it saw. Both
//! submit hints here. A single applier thread takes each hinted path,
//! stats it, and hands the result to `item_ops`. Because the stat
//! happens immediately before the write, on one thread, a stale
//! observation can never overwrite a newer one, and the kind of event
//! that produced the hint does not matter: what is on disk now is the
//! truth.
//!
//! Consequences of "stat is truth":
//!
//! - A path that no longer exists becomes `Gone`, which tombstones the
//!   item and, for a folder, its whole subtree.
//! - A temp file created and deleted before its hint is applied writes
//!   nothing.
//! - An atomic save (write temp, rename over the original) is a plain
//!   modification of the original path.
//!
//! Nothing is recorded as gone while its root directory is missing or
//! empty. That is what an unmounted volume looks like, and a path under
//! an unmounted volume "does not exist" without having been deleted.
//! The reconcile walk has the same rule; only a walk the user confirmed
//! gets past it.
//!
//! A path that matches its root's ignore patterns is treated as not
//! being on disk, whoever hinted it. This is the one place the patterns
//! are enforced; the watcher and the walk filter earlier only to save
//! work. An item tracked before a pattern was added is therefore marked
//! gone the next time its path is hinted, which the next walk does.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use once_cell::sync::Lazy;
use rusqlite::{Connection, OptionalExtension};

use crate::db::Database;
use crate::error::FsPulseError;
use crate::ignore::IgnoreSet;
use crate::item_identity::{Access, VersionSource};
use crate::item_ops::{self, BatchStats, ObservedEvent, ObservedItem};

/// Maximum hints applied in one transaction.
const APPLY_BATCH_SIZE: usize = 2000;

/// Capacity of the hint queue. Submitting blocks when it is full, which
/// is the backpressure that keeps a fast walk from outrunning the
/// database.
const HINT_QUEUE_CAPACITY: usize = 10_000;

/// How long the applier waits for more hints before applying a partial
/// batch.
const BATCH_LINGER: Duration = Duration::from_millis(100);

/// A path whose state on disk may differ from what the database records.
#[derive(Debug, Clone)]
pub struct Hint {
    pub root_id: i64,
    pub path: PathBuf,
    pub source: VersionSource,
    /// The user has confirmed that what is missing really was deleted,
    /// so the empty-root check below does not apply to this hint.
    pub confirmed: bool,
}

/// Stat a hinted path and describe what is there now. Returns `None`
/// when the path cannot be examined for a reason that says nothing
/// about its existence (an unexpected I/O error); the hint is dropped
/// and the next reconcile walk will see the path again.
pub fn observe(hint: &Hint) -> Result<Option<ObservedEvent>, FsPulseError> {
    let gone = || {
        Some(ObservedEvent::Gone {
            root_id: hint.root_id,
            path: hint.path.to_string_lossy().into_owned(),
            source: hint.source,
        })
    };

    let (metadata, access) = match fs::symlink_metadata(&hint.path) {
        Ok(m) if m.is_dir() => match fs::read_dir(&hint.path) {
            Ok(_) => (Some(m), Access::Ok),
            Err(e) if e.kind() == ErrorKind::PermissionDenied => (Some(m), Access::ReadError),
            Err(e) if is_absent(&e) => return Ok(gone()),
            Err(e) => {
                log::warn!("applier: cannot list '{}': {e}", hint.path.display());
                return Ok(None);
            }
        },
        Ok(m) => (Some(m), Access::Ok),
        Err(e) if is_absent(&e) => return Ok(gone()),
        Err(e) if e.kind() == ErrorKind::PermissionDenied => (None, Access::MetaError),
        Err(e) => {
            log::warn!("applier: cannot stat '{}': {e}", hint.path.display());
            return Ok(None);
        }
    };

    let item = ObservedItem::new(&hint.path, metadata.as_ref(), access, hint.source)?;
    Ok(Some(ObservedEvent::Upsert {
        root_id: hint.root_id,
        item,
    }))
}

/// True for errors that mean nothing exists at the path: it is missing,
/// or one of its ancestors is no longer a directory.
fn is_absent(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
}

/// A root's path and ignore patterns, loaded once per batch.
struct RootRules {
    path: PathBuf,
    ignore: IgnoreSet,
    /// Whether the root directory exists and has at least one entry.
    /// Checked at most once per batch, the first time something in the
    /// root looks gone.
    has_content: Option<bool>,
}

impl RootRules {
    fn has_content(&mut self) -> bool {
        *self.has_content.get_or_insert_with(|| {
            let has_content = fs::read_dir(&self.path)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(false);
            if !has_content {
                log::warn!(
                    "applier: root '{}' is missing or empty; not recording anything in it as gone",
                    self.path.display()
                );
            }
            has_content
        })
    }
}

fn load_root_rules(conn: &Connection, root_id: i64) -> Result<Option<RootRules>, FsPulseError> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT root_path, ignore_patterns FROM roots WHERE root_id = ?",
            [root_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(row.map(|(path, patterns)| RootRules {
        path: PathBuf::from(path),
        ignore: IgnoreSet::from_json_lenient(patterns.as_deref(), root_id),
        has_content: None,
    }))
}

fn observe_all(conn: &Connection, hints: &[Hint]) -> Result<Vec<ObservedEvent>, FsPulseError> {
    let mut rules: HashMap<i64, Option<RootRules>> = HashMap::new();
    let mut events = Vec::with_capacity(hints.len());

    for hint in hints {
        if let std::collections::hash_map::Entry::Vacant(slot) = rules.entry(hint.root_id) {
            slot.insert(load_root_rules(conn, hint.root_id)?);
        }
        // A hint for a root that no longer exists is dropped.
        let Some(Some(root)) = rules.get_mut(&hint.root_id) else {
            continue;
        };

        let event = if root.ignore.is_ignored(&root.path, &hint.path) {
            Some(ObservedEvent::Gone {
                root_id: hint.root_id,
                path: hint.path.to_string_lossy().into_owned(),
                source: hint.source,
            })
        } else {
            observe(hint)?
        };

        match event {
            Some(ObservedEvent::Gone { .. }) if !hint.confirmed && !root.has_content() => {}
            Some(event) => events.push(event),
            None => {}
        }
    }
    Ok(events)
}

/// Stat and apply a batch of hints using a pooled connection.
fn apply_hints(hints: &[Hint]) -> Result<BatchStats, FsPulseError> {
    let events = {
        let conn = Database::get_connection()?;
        observe_all(&conn, hints)?
    };
    if events.is_empty() {
        return Ok(BatchStats::default());
    }
    item_ops::apply_observed_events(&events)
}

/// Stat and apply a batch of hints on a caller-supplied connection with
/// an explicit `now`. For tests, which drive time and use an in-memory
/// database.
#[cfg(test)]
pub(crate) fn apply_hints_at(
    conn: &Connection,
    hints: &[Hint],
    now: i64,
) -> Result<BatchStats, FsPulseError> {
    let events = observe_all(conn, hints)?;
    item_ops::apply_observed_events_at(conn, &events, now)
}

// ============================================================================
// The applier thread
// ============================================================================

enum Message {
    Hint(Hint),
    /// Apply everything queued before this message, then reply with the
    /// counts (or the first error) since the previous barrier.
    Barrier(Sender<Result<BatchStats, String>>),
}

static QUEUE: Lazy<Sender<Message>> = Lazy::new(|| {
    let (tx, rx) = bounded(HINT_QUEUE_CAPACITY);
    std::thread::Builder::new()
        .name("applier".into())
        .spawn(move || run(rx))
        .expect("failed to start applier thread");
    tx
});

/// Handle to the process-wide applier thread, started on first use.
pub struct Applier;

impl Applier {
    /// Queue a hint. Blocks while the queue is full.
    pub fn submit(hint: Hint) -> Result<(), FsPulseError> {
        QUEUE
            .send(Message::Hint(hint))
            .map_err(|_| FsPulseError::Error("applier thread is not running".into()))
    }

    /// Wait until every hint queued so far has been applied. Returns the
    /// counts accumulated since the previous barrier, or the first
    /// database error hit in that span.
    ///
    /// The counts cover all producers, not just the caller. They are
    /// exact for a reconcile walk only while it is the sole producer.
    pub fn barrier() -> Result<BatchStats, FsPulseError> {
        let (reply_tx, reply_rx) = bounded(1);
        QUEUE
            .send(Message::Barrier(reply_tx))
            .map_err(|_| FsPulseError::Error("applier thread is not running".into()))?;
        reply_rx
            .recv()
            .map_err(|_| FsPulseError::Error("applier thread stopped before replying".into()))?
            .map_err(FsPulseError::Error)
    }
}

fn run(rx: Receiver<Message>) {
    let mut pending: Vec<Hint> = Vec::with_capacity(APPLY_BATCH_SIZE);
    let mut since_barrier = BatchStats::default();
    let mut first_error: Option<String> = None;

    let flush = |pending: &mut Vec<Hint>,
                     since_barrier: &mut BatchStats,
                     first_error: &mut Option<String>| {
        if pending.is_empty() {
            return;
        }
        match apply_hints(pending) {
            Ok(stats) => since_barrier.absorb(&stats),
            Err(e) => {
                // The batch is dropped. The paths are still wrong in the
                // database; the next reconcile walk finds them again.
                log::error!("applier: failed to apply {} hints: {e}", pending.len());
                first_error.get_or_insert_with(|| e.to_string());
            }
        }
        pending.clear();
    };

    loop {
        // Block for the first message, then linger briefly so hints that
        // arrive close together share a transaction.
        let message = if pending.is_empty() {
            match rx.recv() {
                Ok(m) => m,
                Err(_) => break,
            }
        } else {
            match rx.recv_timeout(BATCH_LINGER) {
                Ok(m) => m,
                Err(RecvTimeoutError::Timeout) => {
                    flush(&mut pending, &mut since_barrier, &mut first_error);
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        };

        match message {
            Message::Hint(hint) => {
                pending.push(hint);
                if pending.len() >= APPLY_BATCH_SIZE {
                    flush(&mut pending, &mut since_barrier, &mut first_error);
                }
            }
            Message::Barrier(reply) => {
                flush(&mut pending, &mut since_barrier, &mut first_error);
                let result = match first_error.take() {
                    Some(e) => Err(e),
                    None => Ok(std::mem::take(&mut since_barrier)),
                };
                let _ = reply.send(result);
            }
        }
    }

    flush(&mut pending, &mut since_barrier, &mut first_error);
}
