pub use sea_orm_migration::prelude::*;

mod m20260218_095047_create_users_table;
mod m20260218_095048_create_user_tokens_table;
mod m20260218_095049_create_trackers_table;
mod m20260219_000000_create_pings_table;
mod m20260630_000000_add_lost_fields;
mod m20260703_000000_add_ping_uuid;
mod m20260704_000000_add_tracker_target_url;
mod m20260704_000001_create_scans_table;
mod m20260704_000002_pings_optional_coords;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260218_095047_create_users_table::Migration),
            Box::new(m20260218_095048_create_user_tokens_table::Migration),
            Box::new(m20260218_095049_create_trackers_table::Migration),
            Box::new(m20260219_000000_create_pings_table::Migration),
            Box::new(m20260630_000000_add_lost_fields::Migration),
            Box::new(m20260703_000000_add_ping_uuid::Migration),
            Box::new(m20260704_000000_add_tracker_target_url::Migration),
            Box::new(m20260704_000001_create_scans_table::Migration),
            Box::new(m20260704_000002_pings_optional_coords::Migration),
        ]
    }
}
