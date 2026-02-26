use log::warn;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{error::FsPulseError, item_version::ItemVersion, utils::Utils};

#[repr(i64)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ItemType {
    File = 0,
    Directory = 1,
    Symlink = 2,
    Unknown = 3,
}

impl ItemType {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => ItemType::File,
            1 => ItemType::Directory,
            2 => ItemType::Symlink,
            3 => ItemType::Unknown,
            _ => {
                warn!(
                    "Invalid ItemType value in database: {}, defaulting to Unknown",
                    value
                );
                ItemType::Unknown
            }
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            ItemType::File => "F",
            ItemType::Directory => "D",
            ItemType::Symlink => "S",
            ItemType::Unknown => "U",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            ItemType::File => "File",
            ItemType::Directory => "Directory",
            ItemType::Symlink => "Symlink",
            ItemType::Unknown => "Unknown",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "FILE" => Some(ItemType::File),
            "DIRECTORY" | "DIR" => Some(ItemType::Directory),
            "SYMLINK" => Some(ItemType::Symlink),
            "UNKNOWN" => Some(ItemType::Unknown),
            // Short names
            "F" => Some(ItemType::File),
            "D" => Some(ItemType::Directory),
            "S" => Some(ItemType::Symlink),
            "U" => Some(ItemType::Unknown),
            _ => None,
        }
    }
}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

#[repr(i64)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access {
    Ok = 0,        // No known access issues (default)
    MetaError = 1, // Can't stat (found during scan phase)
    ReadError = 2, // Can stat, can't read (found during analysis phase)
}

impl Access {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => Access::Ok,
            1 => Access::MetaError,
            2 => Access::ReadError,
            _ => {
                warn!(
                    "Invalid Access value in database: {}, defaulting to Ok",
                    value
                );
                Access::Ok
            }
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            Access::Ok => "N",
            Access::MetaError => "M",
            Access::ReadError => "R",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            Access::Ok => "No Error",
            Access::MetaError => "Meta Error",
            Access::ReadError => "Read Error",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "NO ERROR" | "NOERROR" => Some(Access::Ok),
            "META ERROR" | "METAERROR" => Some(Access::MetaError),
            "READ ERROR" | "READERROR" => Some(Access::ReadError),
            // Short names
            "N" => Some(Access::Ok),
            "M" => Some(Access::MetaError),
            "R" => Some(Access::ReadError),
            _ => None,
        }
    }
}

impl std::fmt::Display for Access {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

/// Stable identity for an item in the new temporal model.
///
/// Maps to the `items` table. Identity columns (root_id, item_path, item_type) live
/// only here — `item_versions` references items via `item_id`.
pub struct ItemIdentity;

impl ItemIdentity {
    /// Insert a new item identity. Computes `item_name` from `path`.
    /// Returns the new item_id.
    pub fn insert(
        conn: &Connection,
        root_id: i64,
        path: &str,
        item_type: ItemType,
    ) -> Result<i64, FsPulseError> {
        let item_name = Utils::display_path_name(path);

        conn.execute(
            "INSERT INTO items (root_id, item_path, item_name, item_type)
             VALUES (?, ?, ?, ?)",
            params![root_id, path, item_name, item_type.as_i64()],
        )?;

        let item_id: i64 = conn.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;
        Ok(item_id)
    }
}

/// An existing item's identity combined with its current version.
///
/// Used by the walk phase to look up an item by (root_id, path, type) and get
/// both the item_id and the current version state in a single JOIN query,
/// eliminating the redundant `ItemVersion::get_current` calls.
pub struct ExistingItem {
    pub item_id: i64,
    pub version: ItemVersion,
}

impl ExistingItem {
    /// Look up an existing item by (root_id, path, type) and return its identity
    /// plus current version. Returns None if the item doesn't exist.
    pub fn get_by_root_path_type(
        conn: &Connection,
        root_id: i64,
        path: &str,
        item_type: ItemType,
    ) -> Result<Option<Self>, FsPulseError> {
        conn.query_row(
            "SELECT iv.version_id, iv.first_scan_id, iv.last_scan_id, iv.is_deleted, iv.access,
                    iv.mod_date, iv.size, iv.file_hash, iv.val, iv.val_error,
                    iv.last_hash_scan, iv.last_val_scan,
                    iv.add_count, iv.modify_count, iv.delete_count,
                    i.item_id
             FROM items i
             JOIN item_versions iv ON iv.item_id = i.item_id
               AND iv.first_scan_id = (
                   SELECT MAX(first_scan_id) FROM item_versions WHERE item_id = i.item_id
               )
             WHERE i.root_id = ? AND i.item_path = ? AND i.item_type = ?",
            params![root_id, path, item_type.as_i64()],
            |row| {
                let version = ItemVersion::from_row(row)?;
                let item_id: i64 = row.get(15)?;
                Ok(ExistingItem { item_id, version })
            },
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }
}
