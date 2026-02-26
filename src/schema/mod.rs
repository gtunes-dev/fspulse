mod base;
mod v10_to_v11;
mod v11_to_v12;
mod v12_to_v13;
mod v13_to_v14;
mod v14_to_v15;
mod v15_to_v16;
mod v16_to_v17;
mod v17_to_v18;
mod v18_to_v19;
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
use v13_to_v14::{migrate_13_to_14, UPGRADE_13_TO_14_POST_SQL, UPGRADE_13_TO_14_PRE_SQL};
use v14_to_v15::{migrate_14_to_15, UPGRADE_14_TO_15_POST_SQL, UPGRADE_14_TO_15_PRE_SQL};
use v15_to_v16::{migrate_15_to_16, UPGRADE_15_TO_16_POST_SQL, UPGRADE_15_TO_16_PRE_SQL};
use v16_to_v17::{migrate_16_to_17, UPGRADE_16_TO_17_POST_SQL, UPGRADE_16_TO_17_PRE_SQL};
use v17_to_v18::{migrate_17_to_18, UPGRADE_17_TO_18_POST_SQL, UPGRADE_17_TO_18_PRE_SQL};
use v18_to_v19::run_backfill_folder_counts;
use v2_to_v3::UPGRADE_2_TO_3_SQL;
use v3_to_v4::UPGRADE_3_TO_4_SQL;
use v4_to_v5::UPGRADE_4_TO_5_SQL;
use v5_to_v6::UPGRADE_5_TO_6_SQL;
use v6_to_v7::UPGRADE_6_TO_7_SQL;
use v7_to_v8::UPGRADE_7_TO_8_SQL;
use v8_to_v9::UPGRADE_8_TO_9_SQL;
use v9_to_v10::UPGRADE_9_TO_10_SQL;

/// Migration descriptor with two variants:
///
/// - **Transacted**: All phases (pre_sql, code_fn, post_sql) run inside a single
///   IMMEDIATE transaction. Used for DDL changes and data transforms that can
///   complete in one transaction.
///
/// - **Standalone**: The code_fn manages its own transactions and is responsible
///   for bumping the schema version (typically in the same transaction as its
///   final cleanup). Used for long-running data migrations that need multiple
///   transactions (e.g., backfilling historical data across many scans).
///   The code_fn must be idempotent for crash recovery — if the process dies
///   mid-run, the schema version hasn't been bumped yet, so the migration
///   re-runs on restart.
pub enum Migration {
    Transacted {
        pre_sql: Option<&'static str>,
        code_fn: Option<MigrationFn>,
        post_sql: Option<&'static str>,
    },
    Standalone {
        code_fn: MigrationFn,
    },
}

impl Migration {
    /// Create a SQL-only migration (no Rust code needed)
    pub const fn sql_only(sql: &'static str) -> Self {
        Self::Transacted {
            pre_sql: Some(sql),
            code_fn: None,
            post_sql: None,
        }
    }

    /// Create a standalone migration where code manages its own transactions.
    pub const fn standalone(code_fn: MigrationFn) -> Self {
        Self::Standalone { code_fn }
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
pub const MIGRATION_13_TO_14: Migration = Migration::Transacted {
    pre_sql: Some(UPGRADE_13_TO_14_PRE_SQL),
    code_fn: Some(migrate_13_to_14),
    post_sql: Some(UPGRADE_13_TO_14_POST_SQL),
};
pub const MIGRATION_14_TO_15: Migration = Migration::Transacted {
    pre_sql: Some(UPGRADE_14_TO_15_PRE_SQL),
    code_fn: Some(migrate_14_to_15),
    post_sql: Some(UPGRADE_14_TO_15_POST_SQL),
};
pub const MIGRATION_15_TO_16: Migration = Migration::Transacted {
    pre_sql: Some(UPGRADE_15_TO_16_PRE_SQL),
    code_fn: Some(migrate_15_to_16),
    post_sql: Some(UPGRADE_15_TO_16_POST_SQL),
};
pub const MIGRATION_16_TO_17: Migration = Migration::Transacted {
    pre_sql: Some(UPGRADE_16_TO_17_PRE_SQL),
    code_fn: Some(migrate_16_to_17),
    post_sql: Some(UPGRADE_16_TO_17_POST_SQL),
};
pub const MIGRATION_17_TO_18: Migration = Migration::Transacted {
    pre_sql: Some(UPGRADE_17_TO_18_PRE_SQL),
    code_fn: Some(migrate_17_to_18),
    post_sql: Some(UPGRADE_17_TO_18_POST_SQL),
};
pub const MIGRATION_18_TO_19: Migration = Migration::standalone(run_backfill_folder_counts);
