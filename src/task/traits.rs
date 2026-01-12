/// Trait for long-running, pausable, stoppable tasks
///
/// Tasks are operations that:
/// - Can be scheduled and queued
/// - Report progress via TaskProgress
/// - Can be paused (checkpoint and resume later)
/// - Can be stopped (rollback and cancel)
///
/// Examples: scanning a root, exporting data, database maintenance
#[allow(dead_code)]
pub trait Task {
    // TODO: Define trait methods as we extract from Scanner
    // Candidates:
    // - fn run(&mut self, progress: &TaskProgress) -> Result<(), Error>
    // - fn can_pause(&self) -> bool
    // - fn can_stop(&self) -> bool
}
