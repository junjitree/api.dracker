use crate::{
    AppState, Error, Response,
    auth::AuthClaim,
    entity::{prelude::Trackers, trackers},
    http::params::QueryParams,
    skippy, util,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, EntityTrait, FromQueryResult,
    IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Select,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::result::Result;

#[derive(Serialize, FromQueryResult)]
struct Dto {
    id: u64,
    slug: Option<String>,
    name: String,
    desc: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
struct TrackerParams {
    #[validate(length(min = 1))]
    name: String,
    desc: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/trackers", get(index))
        .route("/trackers", post(store))
        .route("/trackers/count", get(count))
        .route("/trackers/{id}", get(show))
        .route("/trackers/{id}", put(update))
        .route("/trackers/{id}", delete(destroy))
}

fn query(params: &QueryParams, user_id: u64) -> Select<Trackers> {
    let q = params.q.clone().unwrap_or_default();
    let query = Trackers::find().filter(trackers::Column::UserId.eq(user_id));

    if q.is_empty() {
        return query;
    }

    query.filter(
        Condition::any()
            .add(trackers::Column::Id.eq(&q))
            .add(trackers::Column::Name.contains(&q)),
    )
}

fn query_one(id: u64, user_id: u64) -> Select<Trackers> {
    Trackers::find_by_id(id).filter(trackers::Column::UserId.eq(user_id))
}

fn query_select(query: Select<Trackers>) -> Select<Trackers> {
    query
}

async fn index(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<Vec<Dto>>> {
    let (skip, take) = skippy::skip(params.skip, params.take);
    let col = skippy::column(params.sort.clone(), trackers::Column::UpdatedAt);
    let ord = skippy::order(params.desc, true);

    let mut trackers = query(&params, auth.user_id)
        .offset(skip)
        .limit(take)
        .order_by(col, ord)
        .into_model::<Dto>()
        .all(&state.db)
        .await?;

    let sqids = util::sqids()?;
    for tracker in &mut trackers {
        tracker.slug = Some(sqids.encode(&[tracker.id])?);
    }

    Ok(Json(trackers))
}

async fn count(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<u64>> {
    let count = query(&params, auth.user_id).count(&state.db).await?;

    Ok(Json(count))
}

async fn store(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Json(params): Json<TrackerParams>,
) -> Result<Json<u64>> {
    if let Err(err) = params.validate() {
        return Err(Error::BadRequest(err.to_string()));
    }

    let tracker = trackers::ActiveModel {
        user_id: Set(auth.user_id),
        name: Set(params.name),
        desc: Set(params.desc),

        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    Ok(Json(tracker.id))
}

async fn show(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Dto>> {
    let mut tracker = query_select(query_one(id, auth.user_id))
        .into_model::<Dto>()
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    let sqids = util::sqids()?;
    tracker.slug = Some(sqids.encode(&[tracker.id])?);

    Ok(Json(tracker))
}

async fn update(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(params): Json<TrackerParams>,
) -> Result<Response> {
    if let Err(err) = params.validate() {
        return Err(Error::BadRequest(err.to_string()));
    }

    let tracker = query_select(query_one(id, auth.user_id))
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    let mut tracker = tracker.into_active_model();

    tracker.name = Set(params.name);
    tracker.desc = Set(params.desc);
    tracker.save(&state.db).await?;

    Ok(Response::Accepted)
}

async fn destroy(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Response> {
    let tracker = query_one(id, auth.user_id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    let mut tracker = tracker.into_active_model();
    tracker.updated_at = Set(Utc::now());
    tracker.save(&state.db).await?;

    Ok(Response::Accepted)
}
