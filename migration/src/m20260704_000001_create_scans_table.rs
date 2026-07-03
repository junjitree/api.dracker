use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // The scans table shipped to production out-of-band and was never
        // captured in a migration, so a fresh DB never gets it and every
        // scan read/insert (and the daily prune) fails. Definition mirrors
        // prod's SHOW CREATE TABLE; if_not_exists makes it a no-op there.
        manager
            .create_table(
                Table::create()
                    .table(Scans::Table)
                    .if_not_exists()
                    .col(pk_auto(Scans::Id).big_unsigned())
                    .col(big_unsigned(Scans::TrackerId).not_null())
                    .col(string_len_null(Scans::Ip, 45))
                    .col(string_len_null(Scans::UserAgent, 512))
                    .col(
                        timestamp(Scans::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(
                        Index::create()
                            .name("idx_scan_tracker")
                            .table(Scans::Table)
                            .col(Scans::TrackerId),
                    )
                    .index(
                        Index::create()
                            .name("idx_scan_created_at")
                            .table(Scans::Table)
                            .col(Scans::CreatedAt),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from_tbl(Scans::Table)
                            .from_col(Scans::TrackerId)
                            .to_tbl(Trackers::Table)
                            .to_col(Trackers::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Scans::Table).if_exists().to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Scans {
    Table,
    Id,
    TrackerId,
    Ip,
    UserAgent,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Trackers {
    Table,
    Id,
}
