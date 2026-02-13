use crate::error::FsPulseError;
use crate::scans::{HashMode, ValidateMode};
use serde::{Deserialize, Serialize};

/// Settings for a scan task.
///
/// This struct is serialized to JSON and stored in the `task_settings` column
/// of the `tasks` table. Using a typed struct instead of raw JSON provides:
/// - Type safety at compile time
/// - Automatic validation during deserialization
/// - Easy evolution with `#[serde(default)]` for new fields
///
/// # Evolution
/// When adding new fields:
/// 1. Add the field with `#[serde(default)]` or `#[serde(default = "default_fn")]`
/// 2. Existing rows in the database will deserialize correctly with the default value
/// 3. No migration needed for backwards compatibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanSettings {
    pub hash_mode: HashMode,
    pub validate_mode: ValidateMode,
    // Future fields can be added with #[serde(default)] for backwards compatibility:
    // #[serde(default)]
    // pub some_new_option: bool,
}

impl ScanSettings {
    /// Create new scan settings
    pub fn new(hash_mode: HashMode, validate_mode: ValidateMode) -> Self {
        Self {
            hash_mode,
            validate_mode,
        }
    }

    /// Serialize to JSON string for storage in database
    pub fn to_json(&self) -> Result<String, FsPulseError> {
        serde_json::to_string(self)
            .map_err(|e| FsPulseError::Error(format!("Failed to serialize ScanSettings: {}", e)))
    }

    /// Deserialize from JSON string retrieved from database
    pub fn from_json(json: &str) -> Result<Self, FsPulseError> {
        serde_json::from_str(json)
            .map_err(|e| FsPulseError::Error(format!("Failed to deserialize ScanSettings: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_settings_round_trip() {
        let settings = ScanSettings::new(HashMode::New, ValidateMode::All);
        let json = settings.to_json().unwrap();
        let restored = ScanSettings::from_json(&json).unwrap();
        assert_eq!(settings, restored);
    }

    #[test]
    fn test_scan_settings_json_format() {
        let settings = ScanSettings::new(HashMode::All, ValidateMode::None);
        let json = settings.to_json().unwrap();
        // Verify the JSON contains the expected structure
        assert!(json.contains("hash_mode"));
        assert!(json.contains("validate_mode"));
    }

    #[test]
    fn test_scan_settings_deserialize_with_extra_fields() {
        // Test that we can deserialize JSON with extra fields (forward compatibility)
        // This enables adding new fields to ScanSettings without breaking existing data
        let json = r#"{"hash_mode":"All","validate_mode":"None","unknown_field":123}"#;
        let settings = ScanSettings::from_json(json).unwrap();
        assert_eq!(settings.hash_mode, HashMode::All);
        assert_eq!(settings.validate_mode, ValidateMode::None);
    }
}
