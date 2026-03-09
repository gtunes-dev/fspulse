use rusqlite::{params, Connection, OptionalExtension};

use crate::{error::FsPulseError, hash::Hash};

/// Represents the hash integrity state of a file.
/// Stored as integer in the database.
#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashState {
    Valid = 1,
    Suspect = 2,
}

impl HashState {
    pub fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            1 => HashState::Valid,
            2 => HashState::Suspect,
            _ => {
                log::warn!("Invalid HashState value in database: {}, defaulting to Valid", value);
                HashState::Valid
            }
        }
    }
}

/// A single hash observation for a file. Maps to the `hash_versions` table.
///
/// Each row represents a period where a particular hash was observed.
/// `first_scan_id` is when this hash was first computed; `last_scan_id` is
/// extended each time the hash is re-confirmed unchanged.
///
/// Absence of a row for an item means it has never been hashed.
#[allow(dead_code)]
pub struct HashVersion {
    item_id: i64,
    first_scan_id: i64,
    last_scan_id: i64,
    file_hash: String,
    hash_state: HashState,
}

#[allow(dead_code)]
impl HashVersion {
    pub fn item_id(&self) -> i64 {
        self.item_id
    }

    pub fn first_scan_id(&self) -> i64 {
        self.first_scan_id
    }

    pub fn last_scan_id(&self) -> i64 {
        self.last_scan_id
    }

    pub fn file_hash(&self) -> &str {
        &self.file_hash
    }

    pub fn hash_state(&self) -> HashState {
        self.hash_state
    }

    /// Get the most recent hash_version for an item (if any).
    pub fn get_current(
        conn: &Connection,
        item_id: i64,
    ) -> Result<Option<Self>, FsPulseError> {
        conn.query_row(
            "SELECT item_id, first_scan_id, last_scan_id, file_hash, hash_state
             FROM hash_versions
             WHERE item_id = ?
             ORDER BY first_scan_id DESC
             LIMIT 1",
            params![item_id],
            Self::from_row,
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    /// Insert a new hash observation.
    pub fn insert(
        conn: &Connection,
        item_id: i64,
        scan_id: i64,
        file_hash: &str,
        hash_state: HashState,
    ) -> Result<(), FsPulseError> {
        let hash_blob = Hash::hex_to_blob(file_hash);
        conn.execute(
            "INSERT INTO hash_versions (item_id, first_scan_id, last_scan_id, file_hash, hash_state)
             VALUES (?, ?, ?, ?, ?)",
            params![item_id, scan_id, scan_id, hash_blob, hash_state.as_i64()],
        )?;
        Ok(())
    }

    /// Extend the last_scan_id on an existing hash_version (hash re-confirmed unchanged).
    pub fn extend_last_scan(
        conn: &Connection,
        item_id: i64,
        first_scan_id: i64,
        new_last_scan_id: i64,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "UPDATE hash_versions SET last_scan_id = ?
             WHERE item_id = ? AND first_scan_id = ?",
            params![new_last_scan_id, item_id, first_scan_id],
        )?;
        Ok(())
    }

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(HashVersion {
            item_id: row.get(0)?,
            first_scan_id: row.get(1)?,
            last_scan_id: row.get(2)?,
            file_hash: Hash::blob_to_hex(row.get(3)?),
            hash_state: HashState::from_i64(row.get(4)?),
        })
    }
}
