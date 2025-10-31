mod base;
mod v2_to_v3;
mod v3_to_v4;
mod v4_to_v5;
mod v5_to_v6;
mod v6_to_v7;

pub use base::CREATE_SCHEMA_SQL;
pub use v2_to_v3::UPGRADE_2_TO_3_SQL;
pub use v3_to_v4::UPGRADE_3_TO_4_SQL;
pub use v4_to_v5::UPGRADE_4_TO_5_SQL;
pub use v5_to_v6::UPGRADE_5_TO_6_SQL;
pub use v6_to_v7::UPGRADE_6_TO_7_SQL;
