use rusqlite::{params, Connection, OptionalExtension};

use crate::{error::FsPulseError, item_identity::Access};

/// A single temporal version of an item.
///
/// Maps to the `item_versions` table. Each row represents one distinct state of an item.
/// A new row is created only when observable state changes. Identity (path, type, root)
/// comes from JOINing to the `items` table.
///
/// Hash state is stored in `hash_versions` (keyed on item_version_id).
/// Validation state is stored directly on this table (val_scan_id, val_state, val_error).
#[allow(dead_code)]
pub struct ItemVersion {
    version_id: i64,
    first_scan_id: i64,
    last_scan_id: i64,
    is_added: bool,
    is_deleted: bool,
    access: Access,
    mod_date: Option<i64>,
    size: Option<i64>,
    add_count: Option<i64>,
    modify_count: Option<i64>,
    delete_count: Option<i64>,
    unchanged_count: Option<i64>,
}

#[allow(dead_code)]
impl ItemVersion {
    pub fn version_id(&self) -> i64 {
        self.version_id
    }

    pub fn first_scan_id(&self) -> i64 {
        self.first_scan_id
    }

    pub fn last_scan_id(&self) -> i64 {
        self.last_scan_id
    }

    pub fn is_added(&self) -> bool {
        self.is_added
    }

    pub fn is_deleted(&self) -> bool {
        self.is_deleted
    }

    pub fn access(&self) -> Access {
        self.access
    }

    pub fn mod_date(&self) -> Option<i64> {
        self.mod_date
    }

    pub fn size(&self) -> Option<i64> {
        self.size
    }

    pub fn add_count(&self) -> Option<i64> {
        self.add_count
    }

    pub fn modify_count(&self) -> Option<i64> {
        self.modify_count
    }

    pub fn delete_count(&self) -> Option<i64> {
        self.delete_count
    }

    pub fn unchanged_count(&self) -> Option<i64> {
        self.unchanged_count
    }

    /// Get the current (latest) version of an item.
    pub fn get_current(
        conn: &Connection,
        item_id: i64,
    ) -> Result<Option<Self>, FsPulseError> {
        conn.query_row(
            "SELECT version_id, first_scan_id, last_scan_id, is_added, is_deleted, access,
                    mod_date, size,
                    add_count, modify_count, delete_count, unchanged_count
             FROM item_versions
             WHERE item_id = ?
             ORDER BY first_scan_id DESC
             LIMIT 1",
            params![item_id],
            Self::from_row,
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    /// Insert the first version for a newly discovered item.
    ///
    /// `counts` should be `Some((0, 0, 0, 0))` for folders (add, modify, delete, unchanged),
    /// `None` for files.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_initial(
        conn: &Connection,
        item_id: i64,
        root_id: i64,
        scan_id: i64,
        access: Access,
        mod_date: Option<i64>,
        size: Option<i64>,
        counts: Option<(i64, i64, i64, i64)>,
    ) -> Result<(), FsPulseError> {
        let (add_count, modify_count, delete_count, unchanged_count) = match counts {
            Some((a, m, d, u)) => (Some(a), Some(m), Some(d), Some(u)),
            None => (None, None, None, None),
        };
        conn.execute(
            "INSERT INTO item_versions (
                item_id, root_id, first_scan_id, last_scan_id,
                is_added, is_deleted, access, mod_date, size,
                add_count, modify_count, delete_count, unchanged_count
             ) VALUES (?, ?, ?, ?, 1, 0, ?, ?, ?, ?, ?, ?, ?)",
            params![item_id, root_id, scan_id, scan_id, access.as_i64(), mod_date, size,
                    add_count, modify_count, delete_count, unchanged_count],
        )?;
        Ok(())
    }

    /// Insert a new version with all fields specified explicitly.
    ///
    /// `counts` should be `Some((a, m, d, u))` for folders, `None` for files.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_full(
        conn: &Connection,
        item_id: i64,
        root_id: i64,
        scan_id: i64,
        is_added: bool,
        is_deleted: bool,
        access: Access,
        mod_date: Option<i64>,
        size: Option<i64>,
        counts: Option<(i64, i64, i64, i64)>,
    ) -> Result<(), FsPulseError> {
        let (add_count, modify_count, delete_count, unchanged_count) = match counts {
            Some((a, m, d, u)) => (Some(a), Some(m), Some(d), Some(u)),
            None => (None, None, None, None),
        };
        conn.execute(
            "INSERT INTO item_versions (
                item_id, root_id, first_scan_id, last_scan_id,
                is_added, is_deleted, access, mod_date, size,
                add_count, modify_count, delete_count, unchanged_count
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                item_id, root_id, scan_id, scan_id, is_added, is_deleted, access.as_i64(),
                mod_date, size,
                add_count, modify_count, delete_count, unchanged_count,
            ],
        )?;
        Ok(())
    }

    /// Insert a new version when state changes, carrying forward fields from the previous version.
    ///
    /// Used by item modification. The caller provides the new observable state.
    ///
    /// For folders, descendant counts default to "no changes, everyone unchanged":
    /// `(0, 0, 0, prev_alive)`. The scan analysis phase overwrites these if descendants
    /// actually changed.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_with_carry_forward(
        conn: &Connection,
        item_id: i64,
        root_id: i64,
        scan_id: i64,
        is_deleted: bool,
        access: Access,
        mod_date: Option<i64>,
        size: Option<i64>,
        prev: &ItemVersion,
        is_folder: bool,
    ) -> Result<(), FsPulseError> {
        let counts = if is_folder {
            let prev_alive = prev.add_count().unwrap_or(0)
                + prev.modify_count().unwrap_or(0)
                + prev.unchanged_count().unwrap_or(0);
            Some((0, 0, 0, prev_alive))
        } else {
            None
        };
        Self::insert_full(
            conn, item_id, root_id, scan_id, false, is_deleted, access, mod_date, size,
            counts,
        )
    }

    /// Update `last_scan_id` in place for an unchanged item confirmed alive.
    pub fn touch_last_scan(
        conn: &Connection,
        version_id: i64,
        scan_id: i64,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?",
            params![scan_id, version_id],
        )?;
        Ok(())
    }

    pub(crate) fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(ItemVersion {
            version_id: row.get(0)?,
            first_scan_id: row.get(1)?,
            last_scan_id: row.get(2)?,
            is_added: row.get(3)?,
            is_deleted: row.get(4)?,
            access: Access::from_i64(row.get(5)?),
            mod_date: row.get(6)?,
            size: row.get(7)?,
            add_count: row.get(8)?,
            modify_count: row.get(9)?,
            delete_count: row.get(10)?,
            unchanged_count: row.get(11)?,
        })
    }
}

