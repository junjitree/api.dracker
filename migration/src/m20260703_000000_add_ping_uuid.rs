use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Unguessable handle for the finder's follow-up note. Ping ids are
        // sequential, so the public set-note endpoint keys on this uuid
        // instead. Nullable: pre-existing pings never get a note attached
        // (their finder flow is long over), so they simply stay NULL.
        manager
            .alter_table(
                Table::alter()
                    .table(Pings::Table)
                    .add_column(uuid_null(Pings::Uuid))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_uuid")
                    .table(Pings::Table)
                    .col(Pings::Uuid)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_uuid")
                    .table(Pings::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Pings::Table)
                    .drop_column(Pings::Uuid)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Pings {
    Table,
    Uuid,
}
