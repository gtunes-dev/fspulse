use rusqlite::{params, Connection};

use crate::{error::FsPulseError, items::ItemType, utils::Utils};

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
