mod base;
mod v10_to_v11;
mod v11_to_v12;
mod v12_to_v13;
mod v2_to_v3;
mod v3_to_v4;
mod v4_to_v5;
mod v5_to_v6;
mod v6_to_v7;
mod v7_to_v8;
mod v8_to_v9;
mod v9_to_v10;

use crate::error::FsPulseError;
use rusqlite::Connection;

/// Function type for migration code that transforms data during schema upgrades.
pub type MigrationFn = fn(&Connection) -> Result<(), FsPulseError>;

pub use base::CREATE_SCHEMA_SQL;
use v10_to_v11::UPGRADE_10_TO_11_SQL;
use v11_to_v12::UPGRADE_11_TO_12_SQL;
use v12_to_v13::UPGRADE_12_TO_13_SQL;
use v2_to_v3::UPGRADE_2_TO_3_SQL;
use v3_to_v4::UPGRADE_3_TO_4_SQL;
use v4_to_v5::UPGRADE_4_TO_5_SQL;
use v5_to_v6::UPGRADE_5_TO_6_SQL;
use v6_to_v7::UPGRADE_6_TO_7_SQL;
use v7_to_v8::UPGRADE_7_TO_8_SQL;
use v8_to_v9::UPGRADE_8_TO_9_SQL;
use v9_to_v10::UPGRADE_9_TO_10_SQL;

/// Migration descriptor supporting 3-phase migrations:
/// - pre_sql: SQL batch to run before Rust code (optional)
/// - code_fn: Rust function for complex transformations (optional)
/// - post_sql: SQL batch to run after Rust code (optional)
///
/// For simple SQL-only migrations, only pre_sql is needed.
/// For migrations requiring Rust code (e.g., JSON generation from columns),
/// use all three phases as needed.
pub struct Migration {
    pub pre_sql: Option<&'static str>,
    pub code_fn: Option<MigrationFn>,
    pub post_sql: Option<&'static str>,
}

impl Migration {
    /// Create a SQL-only migration (no Rust code needed)
    pub const fn sql_only(sql: &'static str) -> Self {
        Self {
            pre_sql: Some(sql),
            code_fn: None,
            post_sql: None,
        }
    }
}

// Migration constants for each schema version upgrade
pub const MIGRATION_2_TO_3: Migration = Migration::sql_only(UPGRADE_2_TO_3_SQL);
pub const MIGRATION_3_TO_4: Migration = Migration::sql_only(UPGRADE_3_TO_4_SQL);
pub const MIGRATION_4_TO_5: Migration = Migration::sql_only(UPGRADE_4_TO_5_SQL);
pub const MIGRATION_5_TO_6: Migration = Migration::sql_only(UPGRADE_5_TO_6_SQL);
pub const MIGRATION_6_TO_7: Migration = Migration::sql_only(UPGRADE_6_TO_7_SQL);
pub const MIGRATION_7_TO_8: Migration = Migration::sql_only(UPGRADE_7_TO_8_SQL);
pub const MIGRATION_8_TO_9: Migration = Migration::sql_only(UPGRADE_8_TO_9_SQL);
pub const MIGRATION_9_TO_10: Migration = Migration::sql_only(UPGRADE_9_TO_10_SQL);
pub const MIGRATION_10_TO_11: Migration = Migration::sql_only(UPGRADE_10_TO_11_SQL);
pub const MIGRATION_11_TO_12: Migration = Migration::sql_only(UPGRADE_11_TO_12_SQL);
pub const MIGRATION_12_TO_13: Migration = Migration::sql_only(UPGRADE_12_TO_13_SQL);
