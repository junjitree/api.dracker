use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, FromQueryResult, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Select,
};
use serde::Serialize;

use crate::{
    Error, Result,
    auth::AuthClaim,
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

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pings", get(index))
        .route("/pings/{id}", get(show))
        .route("/pings/count", get(count))
}

fn query(params: &QueryParams, user_id: u64) -> Select<Pings> {
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
        .column(pings::Column::UpdatedAt)
        .filter(trackers::Column::UserId.eq(user_id));

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
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<Vec<Dto>>> {
    let (skip, take) = skippy::skip(params.skip, params.take);
    let col = skippy::column(params.sort.clone(), pings::Column::UpdatedAt);
    let ord = skippy::order(params.desc, true);

    let pings = query(&params, auth.user_id)
        .offset(skip)
        .limit(take)
        .order_by(col, ord)
        .into_model::<Dto>()
        .all(&state.db)
        .await?;

    Ok(Json(pings))
}

async fn count(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<u64>> {
    let count = query(&params, auth.user_id).count(&state.db).await?;

    Ok(Json(count))
}

async fn show(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Dto>> {
    let ping = Pings::find_by_id(id)
        .left_join(Trackers)
        .filter(trackers::Column::UserId.eq(auth.user_id))
        .into_model::<Dto>()
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    Ok(Json(ping))
}
