use rusqlite::{named_params, Transaction};

use crate::{database::Database, error::FsPulseError};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AlertType {
    SuspiciousHash,
    InvalidItem,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AlertStatus {
    Open,
    Flagged,
    Dismissed,
}

impl AlertType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertType::SuspiciousHash => "H",
            AlertType::InvalidItem => "I",
        }
    }

    pub fn short_str_to_full(s: &str) -> Result<&str, FsPulseError> {
        match s {
            "H" => Ok("Suspicious Hash"),
            "I" => Ok("Invalid Item"),
            _ => Err(FsPulseError::Error(format!("Invalid alert type: '{s}'"))),
        }
    }
}

impl AlertStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertStatus::Open => "O",
            AlertStatus::Flagged => "F",
            AlertStatus::Dismissed => "D",
        }
    }

    pub fn short_str_to_full(s: &str) -> Result<&str, FsPulseError> {
        match s {
            "O" => Ok("Open"),
            "F" => Ok("Flagged"),
            "D" => Ok("Dismissed"),
            _ => Err(FsPulseError::Error(format!(
                "Invalid alert status: '{s}'"
            ))),
        }
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
                ":alert_type":      AlertType::SuspiciousHash.as_str(),
                ":alert_status":    AlertStatus::Open.as_str(),
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
                ":alert_type":      AlertType::InvalidItem.as_str(),
                ":alert_status":    AlertStatus::Open.as_str(),
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
            set alert_status = :alert_status 
            where alert_id = :alert_id"#;

        db.conn().execute(
            sql,
            named_params! {
                ":alert_status":    new_status.as_str(),
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
    fn test_alert_type_as_str() {
        assert_eq!(AlertType::SuspiciousHash.as_str(), "H");
        assert_eq!(AlertType::InvalidItem.as_str(), "I");
    }
    
    #[test]
    fn test_alert_type_short_str_to_full_valid() {
        assert_eq!(AlertType::short_str_to_full("H").unwrap(), "Suspicious Hash");
        assert_eq!(AlertType::short_str_to_full("I").unwrap(), "Invalid Item");
    }
    
    #[test]
    fn test_alert_type_short_str_to_full_invalid() {
        let result = AlertType::short_str_to_full("X");
        assert!(result.is_err());
        if let Err(FsPulseError::Error(msg)) = result {
            assert_eq!(msg, "Invalid alert type: 'X'");
        } else {
            panic!("Expected FsPulseError::Error");
        }
        
        // Test empty string
        let result = AlertType::short_str_to_full("");
        assert!(result.is_err());
        if let Err(FsPulseError::Error(msg)) = result {
            assert_eq!(msg, "Invalid alert type: ''");
        } else {
            panic!("Expected FsPulseError::Error");
        }
    }
    
    #[test]
    fn test_alert_type_round_trip_conversion() {
        let types = [AlertType::SuspiciousHash, AlertType::InvalidItem];
        
        for alert_type in types {
            let short_str = alert_type.as_str();
            let full_str = AlertType::short_str_to_full(short_str).unwrap();
            
            // Verify the conversion works and produces expected full strings
            match alert_type {
                AlertType::SuspiciousHash => assert_eq!(full_str, "Suspicious Hash"),
                AlertType::InvalidItem => assert_eq!(full_str, "Invalid Item"),
            }
        }
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
    fn test_alert_status_as_str() {
        assert_eq!(AlertStatus::Open.as_str(), "O");
        assert_eq!(AlertStatus::Flagged.as_str(), "F");
        assert_eq!(AlertStatus::Dismissed.as_str(), "D");
    }
    
    #[test]
    fn test_alert_status_short_str_to_full_valid() {
        assert_eq!(AlertStatus::short_str_to_full("O").unwrap(), "Open");
        assert_eq!(AlertStatus::short_str_to_full("F").unwrap(), "Flagged");
        assert_eq!(AlertStatus::short_str_to_full("D").unwrap(), "Dismissed");
    }
    
    #[test]
    fn test_alert_status_short_str_to_full_invalid() {
        let result = AlertStatus::short_str_to_full("X");
        assert!(result.is_err());
        if let Err(FsPulseError::Error(msg)) = result {
            assert_eq!(msg, "Invalid alert status: 'X'");
        } else {
            panic!("Expected FsPulseError::Error");
        }
        
        // Test empty string
        let result = AlertStatus::short_str_to_full("");
        assert!(result.is_err());
        if let Err(FsPulseError::Error(msg)) = result {
            assert_eq!(msg, "Invalid alert status: ''");
        } else {
            panic!("Expected FsPulseError::Error");
        }
    }
    
    #[test]
    fn test_alert_status_round_trip_conversion() {
        let statuses = [AlertStatus::Open, AlertStatus::Flagged, AlertStatus::Dismissed];
        
        for status in statuses {
            let short_str = status.as_str();
            let full_str = AlertStatus::short_str_to_full(short_str).unwrap();
            
            // Verify the conversion works and produces expected full strings
            match status {
                AlertStatus::Open => assert_eq!(full_str, "Open"),
                AlertStatus::Flagged => assert_eq!(full_str, "Flagged"),
                AlertStatus::Dismissed => assert_eq!(full_str, "Dismissed"),
            }
        }
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
            let short_str = alert_type.as_str();
            assert!(!short_str.is_empty(), "Short string should not be empty");
            assert_eq!(short_str.len(), 1, "Short string should be single character");
        }
        
        // Verify all short strings can be converted back
        assert!(AlertType::short_str_to_full("H").is_ok());
        assert!(AlertType::short_str_to_full("I").is_ok());
    }
    
    #[test]
    fn test_alert_status_completeness() {
        // Verify we can convert all enum variants to strings
        let all_statuses = [AlertStatus::Open, AlertStatus::Flagged, AlertStatus::Dismissed];
        
        for status in all_statuses {
            let short_str = status.as_str();
            assert!(!short_str.is_empty(), "Short string should not be empty");
            assert_eq!(short_str.len(), 1, "Short string should be single character");
        }
        
        // Verify all short strings can be converted back
        assert!(AlertStatus::short_str_to_full("O").is_ok());
        assert!(AlertStatus::short_str_to_full("F").is_ok());
        assert!(AlertStatus::short_str_to_full("D").is_ok());
    }
    
    #[test]
    fn test_alert_type_case_sensitivity() {
        // Verify that case matters for the conversion
        assert!(AlertType::short_str_to_full("h").is_err());
        assert!(AlertType::short_str_to_full("i").is_err());
    }
    
    #[test]
    fn test_alert_status_case_sensitivity() {
        // Verify that case matters for the conversion
        assert!(AlertStatus::short_str_to_full("o").is_err());
        assert!(AlertStatus::short_str_to_full("f").is_err());
        assert!(AlertStatus::short_str_to_full("d").is_err());
    }
}
