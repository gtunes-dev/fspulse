use log::warn;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Change {
    pub change_id: i64,
    pub scan_id: i64,
    pub item_id: i64,
    pub change_type: ChangeType,
    pub is_undelete: Option<bool>, // Present if "A". True if add is undelete
    pub meta_change: Option<bool>, // Present if "M". True if metadata changed, else False
    pub mod_date_old: Option<i64>, // Meaningful if undelete or meta_change
    pub mod_date_new: Option<i64>, // Meaningful if metdata_changed
    pub size_old: Option<i64>, // Meaningful if undelete or meta_change
    pub size_new: Option<i64>, // Meaningful if undelete or meta_change
    pub hash_change: Option<bool>, // Present if "M". True if hash changed, else False
    #[allow(dead_code)]
    pub last_hash_scan_old: Option<i64>, // Present if "M" and hash_change
    pub hash_old: Option<String>,  // Meaningful if undelete or hash_change
    #[allow(dead_code)]
    pub hash_new: Option<String>, // Meaningful if hash_change
    pub val_change: Option<bool>,  // Present if "M", True if validation changed, else False
    #[allow(dead_code)]
    pub last_val_scan_old: Option<i64>, // Present if "M" and validation changed
    pub val_old: Option<i64>,   // Validation state of the item if val_change = true
    #[allow(dead_code)]
    pub val_new: Option<i64>, // Meaningful if undelete or val_change
    #[allow(dead_code)]
    pub val_error_old: Option<String>, // Meaningful if undelete or val_change
    #[allow(dead_code)]
    pub val_error_new: Option<String>, // Meaningful if validity changed

    // $TODO: Remove this. Was a bad idea to have this in the first place
    // Changes should be a simple struct that models a Changes entity
    // Additional non-entity fields
    pub item_type: String,
    pub item_path: String,
}

#[repr(i64)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    NoChange = 0,
    Add = 1,
    Modify = 2,
    Delete = 3,
}

impl ChangeType {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => ChangeType::NoChange,
            1 => ChangeType::Add,
            2 => ChangeType::Modify,
            3 => ChangeType::Delete,
            _ => {
                warn!("Invalid ChangeType value in database: {}, defaulting to NoChange", value);
                ChangeType::NoChange
            }
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            ChangeType::NoChange => "N",
            ChangeType::Add => "A",
            ChangeType::Modify => "M",
            ChangeType::Delete => "D",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            ChangeType::NoChange => "No Change",
            ChangeType::Add => "Add",
            ChangeType::Modify => "Modify",
            ChangeType::Delete => "Delete",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "NO CHANGE" | "NOCHANGE" => Some(ChangeType::NoChange),
            "ADD" => Some(ChangeType::Add),
            "MODIFY" => Some(ChangeType::Modify),
            "DELETE" => Some(ChangeType::Delete),
            // Short names
            "N" => Some(ChangeType::NoChange),
            "A" => Some(ChangeType::Add),
            "M" => Some(ChangeType::Modify),
            "D" => Some(ChangeType::Delete),
            _ => None,
        }
    }
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

impl crate::query::QueryEnum for ChangeType {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|change_type| change_type.as_i64())
    }
}

impl Change {
    // TODO: Implement accessors for other fields
    #[allow(dead_code)]
    pub fn hash_old(&self) -> Option<&str> {
        self.hash_old.as_deref()
    }
    #[allow(dead_code)]
    pub fn val_old(&self) -> Option<i64> {
        self.val_old
    }
    #[allow(dead_code)]
    pub fn val_new(&self) -> Option<i64> {
        self.val_new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_type_integer_values() {
        // Verify the integer values match the expected order
        assert_eq!(ChangeType::NoChange.as_i64(), 0);
        assert_eq!(ChangeType::Add.as_i64(), 1);
        assert_eq!(ChangeType::Modify.as_i64(), 2);
        assert_eq!(ChangeType::Delete.as_i64(), 3);
    }

    #[test]
    fn test_change_type_from_i64() {
        // Verify round-trip conversion
        assert_eq!(ChangeType::from_i64(0), ChangeType::NoChange);
        assert_eq!(ChangeType::from_i64(1), ChangeType::Add);
        assert_eq!(ChangeType::from_i64(2), ChangeType::Modify);
        assert_eq!(ChangeType::from_i64(3), ChangeType::Delete);

        // Invalid values should default to NoChange
        assert_eq!(ChangeType::from_i64(999), ChangeType::NoChange);
        assert_eq!(ChangeType::from_i64(-1), ChangeType::NoChange);
    }

    #[test]
    fn test_change_type_short_name() {
        assert_eq!(ChangeType::Add.short_name(), "A");
        assert_eq!(ChangeType::Delete.short_name(), "D");
        assert_eq!(ChangeType::Modify.short_name(), "M");
        assert_eq!(ChangeType::NoChange.short_name(), "N");
    }
    
    #[test]
    fn test_change_type_display() {
        assert_eq!(ChangeType::Add.to_string(), "Add");
        assert_eq!(ChangeType::Delete.to_string(), "Delete");
        assert_eq!(ChangeType::Modify.to_string(), "Modify");
        assert_eq!(ChangeType::NoChange.to_string(), "No Change");
    }
    
    #[test]
    fn test_change_type_from_string() {
        assert_eq!(ChangeType::from_string("A"), Some(ChangeType::Add));
        assert_eq!(ChangeType::from_string("D"), Some(ChangeType::Delete));
        assert_eq!(ChangeType::from_string("M"), Some(ChangeType::Modify));
        assert_eq!(ChangeType::from_string("N"), Some(ChangeType::NoChange));
        assert_eq!(ChangeType::from_string("Add"), Some(ChangeType::Add));
        assert_eq!(ChangeType::from_string("DELETE"), Some(ChangeType::Delete));

        // Test invalid parse
        assert_eq!(ChangeType::from_string("X"), None);
        assert_eq!(ChangeType::from_string(""), None);
    }

    #[test]
    fn test_change_type_round_trip() {
        let types = [ChangeType::NoChange, ChangeType::Add, ChangeType::Modify, ChangeType::Delete];

        for change_type in types {
            let str_val = change_type.short_name();
            let parsed_back = ChangeType::from_string(str_val).unwrap();
            assert_eq!(change_type, parsed_back, "Round trip failed for {change_type:?}");
        }
    }
    
    #[test]
    fn test_change_type_copy_clone() {
        let change_type = ChangeType::Add;
        let change_type_copy = change_type;
        let change_type_clone = change_type;
        
        // All should be equal
        assert_eq!(change_type, change_type_copy);
        assert_eq!(change_type, change_type_clone);
        assert_eq!(change_type_copy, change_type_clone);
    }
}
