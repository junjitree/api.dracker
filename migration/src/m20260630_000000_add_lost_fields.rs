use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Owner contact lives on the user (one set of details for all their tags).
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(string_null(Users::Phone))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(text_null(Users::Address))
                    .to_owned(),
            )
            .await?;

        // "Lost" state + an optional finder-facing message live on the tracker.
        manager
            .alter_table(
                Table::alter()
                    .table(Trackers::Table)
                    .add_column(boolean(Trackers::IsLost).default(false))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Trackers::Table)
                    .add_column(text_null(Trackers::Message))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Trackers::Table)
                    .drop_column(Trackers::Message)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Trackers::Table)
                    .drop_column(Trackers::IsLost)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::Address)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::Phone)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Phone,
    Address,
}

#[derive(DeriveIden)]
enum Trackers {
    Table,
    IsLost,
    Message,
}
