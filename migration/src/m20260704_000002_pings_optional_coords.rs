use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // A finder can now leave a note without sharing their location, so a
        // ping's coordinates become optional. MODIFY to NULL is idempotent —
        // re-running on an already-nullable column is a no-op.
        manager
            .alter_table(
                Table::alter()
                    .table(Pings::Table)
                    .modify_column(double_null(Pings::Lat))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Pings::Table)
                    .modify_column(double_null(Pings::Lon))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Note-only pings would violate NOT NULL — remove them before tightening.
        manager
            .get_connection()
            .execute_unprepared("DELETE FROM pings WHERE lat IS NULL OR lon IS NULL")
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Pings::Table)
                    .modify_column(double(Pings::Lat))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Pings::Table)
                    .modify_column(double(Pings::Lon))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Pings {
    Table,
    Lat,
    Lon,
}
