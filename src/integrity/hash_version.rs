use rusqlite::{params, Connection, OptionalExtension};

use crate::{error::FsPulseError, hash::Hash};

/// Represents the hash integrity state of a file.
/// Stored as integer in the database.
#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashState {
    Baseline = 1,
    Suspect = 2,
}

impl HashState {
    pub fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            1 => HashState::Baseline,
            2 => HashState::Suspect,
            _ => {
                log::warn!("Invalid HashState value in database: {}, defaulting to Baseline", value);
                HashState::Baseline
            }
        }
    }
}

/// A single hash observation for a file version. Maps to the `hash_versions` table.
///
/// Each row represents a period where a particular hash was observed for an
/// item_version. `first_seen_at` is when this hash was first computed;
/// `last_seen_at` is extended each time the hash is re-confirmed unchanged.
///
/// A version can have zero or more hash observations (a log of hash checks).
/// Multiple rows for the same version track hash changes over time (e.g., bit rot).
#[allow(dead_code)]
pub struct HashVersion {
    item_id: i64,
    item_version: i64,
    first_seen_at: i64,
    last_seen_at: i64,
    file_hash: String,
    hash_state: HashState,
}

#[allow(dead_code)]
impl HashVersion {
    pub fn item_id(&self) -> i64 {
        self.item_id
    }

    pub fn item_version(&self) -> i64 {
        self.item_version
    }

    pub fn first_seen_at(&self) -> i64 {
        self.first_seen_at
    }

    pub fn last_seen_at(&self) -> i64 {
        self.last_seen_at
    }

    pub fn file_hash(&self) -> &str {
        &self.file_hash
    }

    pub fn hash_state(&self) -> HashState {
        self.hash_state
    }

    /// Get the most recent hash_version for an item_version (if any).
    pub fn get_current_for_version(
        conn: &Connection,
        item_id: i64,
        item_version: i64,
    ) -> Result<Option<Self>, FsPulseError> {
        conn.query_row(
            "SELECT item_id, item_version, first_seen_at, last_seen_at, file_hash, hash_state
             FROM hash_versions
             WHERE item_id = ? AND item_version = ?
             ORDER BY first_seen_at DESC
             LIMIT 1",
            params![item_id, item_version],
            Self::from_row,
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    /// Insert a new hash observation.
    pub fn insert(
        conn: &Connection,
        item_id: i64,
        item_version: i64,
        now: i64,
        file_hash: &str,
        hash_state: HashState,
    ) -> Result<(), FsPulseError> {
        let hash_blob = Hash::hex_to_blob(file_hash);
        conn.execute(
            "INSERT INTO hash_versions (item_id, item_version, first_seen_at, last_seen_at, file_hash, hash_state)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![item_id, item_version, now, now, hash_blob, hash_state.as_i64()],
        )?;
        Ok(())
    }

    /// Extend the last_seen_at on an existing hash_version (hash re-confirmed unchanged).
    pub fn extend_last_seen_at(
        conn: &Connection,
        item_id: i64,
        item_version: i64,
        first_seen_at: i64,
        new_last_seen_at: i64,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "UPDATE hash_versions SET last_seen_at = ?
             WHERE item_id = ? AND item_version = ? AND first_seen_at = ?",
            params![new_last_seen_at, item_id, item_version, first_seen_at],
        )?;
        Ok(())
    }

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(HashVersion {
            item_id: row.get(0)?,
            item_version: row.get(1)?,
            first_seen_at: row.get(2)?,
            last_seen_at: row.get(3)?,
            file_hash: Hash::blob_to_hex(row.get(4)?),
            hash_state: HashState::from_i64(row.get(5)?),
        })
    }
}
