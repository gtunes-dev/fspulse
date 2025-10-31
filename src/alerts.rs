use log::warn;
use rusqlite::{named_params, Transaction};
use serde::{Deserialize, Serialize};

use crate::{database::Database, error::FsPulseError};

#[repr(i64)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    SuspiciousHash = 0,
    InvalidItem = 1,
}

#[repr(i64)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertStatus {
    Open = 0,
    Flagged = 1,
    Dismissed = 2,
}

impl AlertType {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => AlertType::SuspiciousHash,
            1 => AlertType::InvalidItem,
            _ => {
                warn!("Invalid AlertType value in database: {}, defaulting to SuspiciousHash", value);
                AlertType::SuspiciousHash
            }
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            AlertType::SuspiciousHash => "H",
            AlertType::InvalidItem => "I",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            AlertType::SuspiciousHash => "Suspicious Hash",
            AlertType::InvalidItem => "Invalid Item",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "SUSPICIOUS HASH" | "SUSPICIOUSHASH" => Some(AlertType::SuspiciousHash),
            "INVALID ITEM" | "INVALIDITEM" => Some(AlertType::InvalidItem),
            // Short names
            "H" => Some(AlertType::SuspiciousHash),
            "I" => Some(AlertType::InvalidItem),
            _ => None,
        }
    }
}

impl std::fmt::Display for AlertType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

impl crate::query::QueryEnum for AlertType {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|alert_type| alert_type.as_i64())
    }
}

impl AlertStatus {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => AlertStatus::Open,
            1 => AlertStatus::Flagged,
            2 => AlertStatus::Dismissed,
            _ => {
                warn!("Invalid AlertStatus value in database: {}, defaulting to Open", value);
                AlertStatus::Open
            }
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            AlertStatus::Open => "O",
            AlertStatus::Flagged => "F",
            AlertStatus::Dismissed => "D",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            AlertStatus::Open => "Open",
            AlertStatus::Flagged => "Flagged",
            AlertStatus::Dismissed => "Dismissed",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "OPEN" => Some(AlertStatus::Open),
            "FLAGGED" => Some(AlertStatus::Flagged),
            "DISMISSED" => Some(AlertStatus::Dismissed),
            // Short names
            "O" => Some(AlertStatus::Open),
            "F" => Some(AlertStatus::Flagged),
            "D" => Some(AlertStatus::Dismissed),
            _ => None,
        }
    }

}

impl std::fmt::Display for AlertStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

impl crate::query::QueryEnum for AlertStatus {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|status| status.as_i64())
    }
}

pub struct Alerts;

impl Alerts {
    pub fn meta_changed_between(
        tx: &Transaction,
        item_id: i64,
        prev_hash_scan: i64,
        current_scan: i64,
    ) -> Result<bool, FsPulseError> {
        let sql = r#"
            SELECT EXISTS (
                SELECT 1
                FROM   changes
                WHERE  item_id      = :item_id
                AND  scan_id     > :prev_scan     -- AFTER previous-hash scan
                AND  scan_id     < :current_scan  -- BEFORE current scan
                AND  meta_change  = 1
            ) AS has_meta_change;
        "#;

        let has_meta_change: bool = tx.query_row(
            sql,
            named_params! {
                ":item_id":      item_id,
                ":prev_scan":    prev_hash_scan,
                ":current_scan": current_scan,
            },
            |row| row.get(0),
        )?;

        Ok(has_meta_change)
    }

    pub fn add_suspicious_hash_alert(
        tx: &Transaction,
        scan_id: i64,
        item_id: i64,
        prev_hash_scan: Option<i64>,
        hash_old: Option<&str>,
        hash_new: &str,
    ) -> Result<(), FsPulseError> {
        let sql = r#"
            INSERT INTO alerts (
              alert_type,
              alert_status,             
              scan_id,
              item_id,
              created_at,
              prev_hash_scan,
              hash_old,
              hash_new
            )
            VALUES (
                :alert_type,
                :alert_status,
                :scan_id,
                :item_id,
                strftime('%s', 'now', 'utc'),
                :prev_hash_scan,
                :hash_old,
                :hash_new
            )
        "#;

        tx.execute(
            sql,
            named_params! {
                ":alert_type":      AlertType::SuspiciousHash.as_i64(),
                ":alert_status":    AlertStatus::Open.as_i64(),
                ":scan_id":         scan_id,
                ":item_id":         item_id,
                ":prev_hash_scan":  prev_hash_scan,
                ":hash_old":        hash_old,
                ":hash_new":        hash_new,
            },
        )?;
        Ok(())
    }

    pub fn add_invalid_item_alert(
        tx: &Transaction,
        scan_id: i64,
        item_id: i64,
        val_error: &str,
    ) -> Result<(), FsPulseError> {
        let sql = r#"
            INSERT INTO alerts (
                alert_type,
                alert_status,
                scan_id,
                item_id,
                created_at,
                val_error
            )
            VALUES (
                :alert_type,
                :alert_status,
                :scan_id,
                :item_id,
                strftime('%s', 'now', 'utc'),
                :val_error
            )
        "#;

        tx.execute(
            sql,
            named_params! {
                ":alert_type":      AlertType::InvalidItem.as_i64(),
                ":alert_status":    AlertStatus::Open.as_i64(),
                ":scan_id":         scan_id,
                ":item_id":         item_id,
                ":val_error":       val_error,
            },
        )?;

        Ok(())
    }

    pub fn set_alert_status(
        db: &Database,
        alert_id: i64,
        new_status: AlertStatus,
    ) -> Result<(), FsPulseError> {
        let sql = r#"
            UPDATE alerts
            SET alert_status = :alert_status,
                updated_at = strftime('%s', 'now', 'utc')
            WHERE alert_id = :alert_id"#;

        db.conn().execute(
            sql,
            named_params! {
                ":alert_status":    new_status.as_i64(),
                ":alert_id":        alert_id,
            },
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_alert_type_integer_values() {
        // Verify the integer values match the expected order
        assert_eq!(AlertType::SuspiciousHash.as_i64(), 0);
        assert_eq!(AlertType::InvalidItem.as_i64(), 1);
    }

    #[test]
    fn test_alert_type_from_i64() {
        // Verify round-trip conversion
        assert_eq!(AlertType::from_i64(0), AlertType::SuspiciousHash);
        assert_eq!(AlertType::from_i64(1), AlertType::InvalidItem);

        // Invalid values should default to SuspiciousHash
        assert_eq!(AlertType::from_i64(999), AlertType::SuspiciousHash);
        assert_eq!(AlertType::from_i64(-1), AlertType::SuspiciousHash);
    }

    #[test]
    fn test_alert_type_short_name() {
        assert_eq!(AlertType::SuspiciousHash.short_name(), "H");
        assert_eq!(AlertType::InvalidItem.short_name(), "I");
    }

    #[test]
    fn test_alert_type_full_name() {
        assert_eq!(AlertType::SuspiciousHash.full_name(), "Suspicious Hash");
        assert_eq!(AlertType::InvalidItem.full_name(), "Invalid Item");
    }
    
    #[test]
    fn test_alert_type_traits() {
        let suspicious = AlertType::SuspiciousHash;
        let invalid = AlertType::InvalidItem;
        
        // Test PartialEq
        assert_eq!(suspicious, AlertType::SuspiciousHash);
        assert_eq!(invalid, AlertType::InvalidItem);
        assert_ne!(suspicious, invalid);
        
        // Test Copy
        let suspicious_copy = suspicious;
        assert_eq!(suspicious, suspicious_copy);
        
        // Test Clone
        let invalid_clone = invalid;
        assert_eq!(invalid, invalid_clone);
        
        // Test Debug (just ensure it doesn't panic)
        let debug_str = format!("{suspicious:?}");
        assert!(debug_str.contains("SuspiciousHash"));
    }
    
    #[test]
    fn test_alert_status_integer_values() {
        // Verify the integer values match the expected order
        assert_eq!(AlertStatus::Open.as_i64(), 0);
        assert_eq!(AlertStatus::Flagged.as_i64(), 1);
        assert_eq!(AlertStatus::Dismissed.as_i64(), 2);
    }

    #[test]
    fn test_alert_status_from_i64() {
        // Verify round-trip conversion
        assert_eq!(AlertStatus::from_i64(0), AlertStatus::Open);
        assert_eq!(AlertStatus::from_i64(1), AlertStatus::Flagged);
        assert_eq!(AlertStatus::from_i64(2), AlertStatus::Dismissed);

        // Invalid values should default to Open
        assert_eq!(AlertStatus::from_i64(999), AlertStatus::Open);
        assert_eq!(AlertStatus::from_i64(-1), AlertStatus::Open);
    }

    #[test]
    fn test_alert_status_short_name() {
        assert_eq!(AlertStatus::Open.short_name(), "O");
        assert_eq!(AlertStatus::Flagged.short_name(), "F");
        assert_eq!(AlertStatus::Dismissed.short_name(), "D");
    }

    #[test]
    fn test_alert_status_full_name() {
        assert_eq!(AlertStatus::Open.full_name(), "Open");
        assert_eq!(AlertStatus::Flagged.full_name(), "Flagged");
        assert_eq!(AlertStatus::Dismissed.full_name(), "Dismissed");
    }
    
    #[test]
    fn test_alert_status_traits() {
        let open = AlertStatus::Open;
        let flagged = AlertStatus::Flagged;
        let dismissed = AlertStatus::Dismissed;
        
        // Test PartialEq
        assert_eq!(open, AlertStatus::Open);
        assert_eq!(flagged, AlertStatus::Flagged);
        assert_eq!(dismissed, AlertStatus::Dismissed);
        assert_ne!(open, flagged);
        assert_ne!(flagged, dismissed);
        assert_ne!(open, dismissed);
        
        // Test Copy
        let open_copy = open;
        assert_eq!(open, open_copy);
        
        // Test Clone
        let flagged_clone = flagged;
        assert_eq!(flagged, flagged_clone);
        
        // Test Debug (just ensure it doesn't panic)
        let debug_str = format!("{dismissed:?}");
        assert!(debug_str.contains("Dismissed"));
    }
    
    #[test]
    fn test_alert_type_completeness() {
        // Verify we can convert all enum variants to strings
        let all_types = [AlertType::SuspiciousHash, AlertType::InvalidItem];

        for alert_type in all_types {
            let short_str = alert_type.short_name();
            assert!(!short_str.is_empty(), "Short string should not be empty");
            assert_eq!(short_str.len(), 1, "Short string should be single character");

            let full_str = alert_type.full_name();
            assert!(!full_str.is_empty(), "Full string should not be empty");
        }
    }

    #[test]
    fn test_alert_status_completeness() {
        // Verify we can convert all enum variants to strings
        let all_statuses = [AlertStatus::Open, AlertStatus::Flagged, AlertStatus::Dismissed];

        for status in all_statuses {
            let short_str = status.short_name();
            assert!(!short_str.is_empty(), "Short string should not be empty");
            assert_eq!(short_str.len(), 1, "Short string should be single character");

            let full_str = status.full_name();
            assert!(!full_str.is_empty(), "Full string should not be empty");
        }
    }
}
