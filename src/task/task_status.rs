use log::warn;
use serde::{Deserialize, Serialize};

/// Task status enum — lifecycle state of a task in the `tasks` table.
///
/// Persisted statuses (0–4) are stored as integers in `tasks.status`.
/// Transient statuses (100+) are in-memory only, used for WebSocket broadcasting
/// during the brief window between a stop/pause request and the task finishing.
#[repr(i64)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    // --- Persisted statuses (stored in tasks.status column) ---
    Pending = 0,   // Queued, waiting to run (run_at determines eligibility)
    Running = 1,   // Currently executing
    Completed = 2, // Finished successfully
    Stopped = 3,   // User requested stop
    Error = 4,     // Task failed

    // --- Transient statuses (in-memory only, never persisted) ---
    Pausing = 100,  // Stop requested, will pause (resume later)
    Stopping = 101, // Stop requested, will stop (no resume)
}

impl TaskStatus {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    #[allow(dead_code)]
    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => TaskStatus::Pending,
            1 => TaskStatus::Running,
            2 => TaskStatus::Completed,
            3 => TaskStatus::Stopped,
            4 => TaskStatus::Error,
            100 => TaskStatus::Pausing,
            101 => TaskStatus::Stopping,
            _ => {
                warn!(
                    "Invalid TaskStatus value in database: {}, defaulting to Pending",
                    value
                );
                TaskStatus::Pending
            }
        }
    }

    #[allow(dead_code)]
    pub fn short_name(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "P",
            TaskStatus::Running => "R",
            TaskStatus::Completed => "C",
            TaskStatus::Stopped => "S",
            TaskStatus::Error => "E",
            TaskStatus::Pausing => "Pa",
            TaskStatus::Stopping => "St",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "Pending",
            TaskStatus::Running => "Running",
            TaskStatus::Completed => "Completed",
            TaskStatus::Stopped => "Stopped",
            TaskStatus::Error => "Error",
            TaskStatus::Pausing => "Pausing",
            TaskStatus::Stopping => "Stopping",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "PENDING" | "P" => Some(TaskStatus::Pending),
            "RUNNING" | "R" => Some(TaskStatus::Running),
            "COMPLETED" | "C" => Some(TaskStatus::Completed),
            "STOPPED" | "S" => Some(TaskStatus::Stopped),
            "ERROR" | "E" => Some(TaskStatus::Error),
            "PAUSING" | "PA" => Some(TaskStatus::Pausing),
            "STOPPING" | "ST" => Some(TaskStatus::Stopping),
            _ => None,
        }
    }

    /// Returns true if this is a terminal status (task is done)
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Stopped | TaskStatus::Error
        )
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

impl crate::query::QueryEnum for TaskStatus {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|status| status.as_i64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_integer_values() {
        assert_eq!(TaskStatus::Pending.as_i64(), 0);
        assert_eq!(TaskStatus::Running.as_i64(), 1);
        assert_eq!(TaskStatus::Completed.as_i64(), 2);
        assert_eq!(TaskStatus::Stopped.as_i64(), 3);
        assert_eq!(TaskStatus::Error.as_i64(), 4);
        assert_eq!(TaskStatus::Pausing.as_i64(), 100);
        assert_eq!(TaskStatus::Stopping.as_i64(), 101);
    }

    #[test]
    fn test_task_status_from_i64() {
        assert_eq!(TaskStatus::from_i64(0), TaskStatus::Pending);
        assert_eq!(TaskStatus::from_i64(1), TaskStatus::Running);
        assert_eq!(TaskStatus::from_i64(2), TaskStatus::Completed);
        assert_eq!(TaskStatus::from_i64(3), TaskStatus::Stopped);
        assert_eq!(TaskStatus::from_i64(4), TaskStatus::Error);
        assert_eq!(TaskStatus::from_i64(100), TaskStatus::Pausing);
        assert_eq!(TaskStatus::from_i64(101), TaskStatus::Stopping);
        // Invalid values should default to Pending
        assert_eq!(TaskStatus::from_i64(999), TaskStatus::Pending);
        assert_eq!(TaskStatus::from_i64(-1), TaskStatus::Pending);
    }

    #[test]
    fn test_task_status_is_terminal() {
        assert!(!TaskStatus::Pending.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Stopped.is_terminal());
        assert!(TaskStatus::Error.is_terminal());
        assert!(!TaskStatus::Pausing.is_terminal());
        assert!(!TaskStatus::Stopping.is_terminal());
    }

    #[test]
    fn test_task_status_serde_roundtrip() {
        let status = TaskStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"running\"");
        let restored: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, restored);

        // Test pending serializes correctly
        let json = serde_json::to_string(&TaskStatus::Pending).unwrap();
        assert_eq!(json, "\"pending\"");
    }

    #[test]
    fn test_task_status_from_string() {
        assert_eq!(TaskStatus::from_string("pending"), Some(TaskStatus::Pending));
        assert_eq!(
            TaskStatus::from_string("RUNNING"),
            Some(TaskStatus::Running)
        );
        assert_eq!(
            TaskStatus::from_string("Completed"),
            Some(TaskStatus::Completed)
        );
        assert_eq!(TaskStatus::from_string("S"), Some(TaskStatus::Stopped));
        assert_eq!(TaskStatus::from_string("E"), Some(TaskStatus::Error));
        assert_eq!(TaskStatus::from_string("invalid"), None);
    }
}
