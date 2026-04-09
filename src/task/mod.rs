mod checkpoint_task;
mod compact_database_task;
mod progress;
mod scan_task;
mod task_status;
mod task_type;
mod traits;

// CheckpointSettings is intentionally not re-exported yet — no caller
// needs it until step 5b wires checkpoint scheduling. It still lives
// in checkpoint_task for protocol-consistency parity with scan and
// compact, and so the empty-settings JSON shape is locked in early.
pub use checkpoint_task::CheckpointTask;
pub use compact_database_task::{CompactDatabaseSettings, CompactDatabaseTask};
pub use progress::{BroadcastMessage, TaskProgress};
pub use scan_task::{AnalysisTracker, ScanSettings, ScanTask, ScanTaskState};
pub use task_status::TaskStatus;
pub use task_type::TaskType;
pub use traits::Task;
