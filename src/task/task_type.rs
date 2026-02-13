use log::warn;
use serde::{Deserialize, Serialize};

/// Task type enum - what kind of task is stored in the tasks table
///
/// This enum is stored as an integer in the database and serialized as
/// lowercase strings for the WebSocket protocol.
#[repr(i64)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskType {
    Scan = 0,
    // Future task types:
    // DatabaseCompact = 1,
    // Export = 2,
    // etc.
}

impl TaskType {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    #[allow(dead_code)] // Part of AlertType pattern, will be used when reading from DB
    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => TaskType::Scan,
            _ => {
                warn!(
                    "Invalid TaskType value in database: {}, defaulting to Scan",
                    value
                );
                TaskType::Scan
            }
        }
    }

    #[allow(dead_code)] // Part of AlertType pattern, will be used for display
    pub fn short_name(&self) -> &'static str {
        match self {
            TaskType::Scan => "S",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            TaskType::Scan => "Scan",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "SCAN" => Some(TaskType::Scan),
            // Short names
            "S" => Some(TaskType::Scan),
            _ => None,
        }
    }
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

impl crate::query::QueryEnum for TaskType {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|task_type| task_type.as_i64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_type_integer_values() {
        assert_eq!(TaskType::Scan.as_i64(), 0);
    }

    #[test]
    fn test_task_type_from_i64() {
        assert_eq!(TaskType::from_i64(0), TaskType::Scan);
        // Invalid values should default to Scan
        assert_eq!(TaskType::from_i64(999), TaskType::Scan);
        assert_eq!(TaskType::from_i64(-1), TaskType::Scan);
    }

    #[test]
    fn test_task_type_short_name() {
        assert_eq!(TaskType::Scan.short_name(), "S");
    }

    #[test]
    fn test_task_type_full_name() {
        assert_eq!(TaskType::Scan.full_name(), "Scan");
    }

    #[test]
    fn test_task_type_from_string() {
        assert_eq!(TaskType::from_string("scan"), Some(TaskType::Scan));
        assert_eq!(TaskType::from_string("SCAN"), Some(TaskType::Scan));
        assert_eq!(TaskType::from_string("Scan"), Some(TaskType::Scan));
        assert_eq!(TaskType::from_string("S"), Some(TaskType::Scan));
        assert_eq!(TaskType::from_string("s"), Some(TaskType::Scan));
        assert_eq!(TaskType::from_string("invalid"), None);
    }

    #[test]
    fn test_task_type_display() {
        assert_eq!(format!("{}", TaskType::Scan), "Scan");
    }

    #[test]
    fn test_task_type_serde_roundtrip() {
        let scan = TaskType::Scan;
        let json = serde_json::to_string(&scan).unwrap();
        // Should serialize to lowercase "scan"
        assert_eq!(json, "\"scan\"");
        let restored: TaskType = serde_json::from_str(&json).unwrap();
        assert_eq!(scan, restored);
    }

    #[test]
    fn test_task_type_traits() {
        let scan = TaskType::Scan;

        // Test Copy
        let scan_copy = scan;
        assert_eq!(scan, scan_copy);

        // Test Clone
        let scan_clone = scan;
        assert_eq!(scan, scan_clone);

        // Test Debug
        let debug_str = format!("{scan:?}");
        assert!(debug_str.contains("Scan"));
    }
}
