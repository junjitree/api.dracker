use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // `target_url` (the optional custom redirect) shipped to production
        // out-of-band and was never captured in a migration, so a fresh DB
        // never gets the column and every tracker query fails. Add it — but
        // guard on has_column so this is a no-op where it already exists
        // (prod), since MySQL can't ADD COLUMN IF NOT EXISTS.
        if !manager.has_column("trackers", "target_url").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Trackers::Table)
                        .add_column(string_len_null(Trackers::TargetUrl, 2048))
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.has_column("trackers", "target_url").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Trackers::Table)
                        .drop_column(Trackers::TargetUrl)
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Trackers {
    Table,
    TargetUrl,
}
