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
            _ => Err(FsPulseError::Error(format!("Invalid alert type: '{}'", s))),
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
                "Invalid alert status: '{}'",
                s
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
