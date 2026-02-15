mod compact_database_task;
mod progress;
mod scan_task;
mod task_status;
mod task_type;
mod traits;

pub use compact_database_task::{CompactDatabaseSettings, CompactDatabaseTask};
pub use progress::{BroadcastMessage, TaskProgress};
pub use scan_task::{AnalysisTracker, ScanSettings, ScanTask, ScanTaskState};
pub use task_status::TaskStatus;
pub use task_type::TaskType;
pub use traits::Task;
