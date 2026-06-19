//! `SeaORM` Entity

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "scans")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: u64,
    pub tracker_id: u64,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::trackers::Entity",
        from = "Column::TrackerId",
        to = "super::trackers::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Trackers,
}

impl Related<super::trackers::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Trackers.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
