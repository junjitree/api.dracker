use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Select,
};
use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    entity::{
        pings,
        prelude::{Pings, Trackers},
        trackers,
    },
    http::params::QueryParams,
    skippy,
    state::AppState,
};

#[derive(Serialize, FromQueryResult)]
struct Dto {
    id: u64,
    tracker_id: u64,
    tracker: Option<String>,
    lat: f64,
    lon: f64,
    note: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct PingParams {
    tracker_id: u64,
    lat: f64,
    lon: f64,
    note: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pings", get(index))
        .route("/pings", post(store))
        .route("/pings/{id}", get(show))
        .route("/pings/count", get(count))
}

fn query(params: &QueryParams) -> Select<Pings> {
    let q = params.q.clone().unwrap_or_default();
    let query = Pings::find()
        .left_join(Trackers)
        .select_only()
        .column(pings::Column::Id)
        .column(pings::Column::TrackerId)
        .column_as(trackers::Column::Name, "tracker")
        .column(pings::Column::Note)
        .column(pings::Column::Lat)
        .column(pings::Column::Lon)
        .column(pings::Column::CreatedAt)
        .column(pings::Column::UpdatedAt);

    if q.is_empty() {
        return query;
    }

    query.filter(
        Condition::any()
            .add(pings::Column::Id.eq(&q))
            .add(pings::Column::TrackerId.eq(&q)),
    )
}

async fn index(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<Vec<Dto>>> {
    let (skip, take) = skippy::skip(params.skip, params.take);
    let col = skippy::column(params.sort.clone(), pings::Column::UpdatedAt);
    let ord = skippy::order(params.desc, true);

    let pings = query(&params)
        .offset(skip)
        .limit(take)
        .order_by(col, ord)
        .into_model::<Dto>()
        .all(&state.db)
        .await?;

    Ok(Json(pings))
}

async fn count(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<u64>> {
    let count = query(&params).count(&state.db).await?;

    Ok(Json(count))
}

async fn store(State(state): State<AppState>, Json(params): Json<PingParams>) -> Result<Json<u64>> {
    let ping = pings::ActiveModel {
        tracker_id: Set(params.tracker_id),
        lat: Set(params.lat),
        lon: Set(params.lon),
        note: Set(params.note),

        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    Ok(Json(ping.id))
}

async fn show(State(state): State<AppState>, Path(id): Path<u64>) -> Result<Json<Dto>> {
    let ping = Pings::find_by_id(id)
        .into_model::<Dto>()
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    Ok(Json(ping))
}
