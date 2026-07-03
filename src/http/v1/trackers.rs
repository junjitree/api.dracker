use crate::{
    AppState, Error, Response,
    auth::AuthClaim,
    entity::{
        pings,
        prelude::{Pings, Scans, Trackers},
        scans, trackers,
    },
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
    IntoActiveModel, JoinType, ModelTrait, Order, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, Select, sea_query::Expr,
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
    target_url: Option<String>,
    is_lost: bool,
    message: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize, FromQueryResult)]
struct ScanDto {
    id: u64,
    ip: Option<String>,
    user_agent: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, FromQueryResult)]
struct PingDto {
    id: u64,
    note: String,
    lat: f64,
    lon: f64,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
struct TrackerParams {
    #[validate(length(min = 1))]
    name: String,
    desc: String,
    #[serde(default)]
    target_url: Option<String>,
    #[serde(default)]
    is_lost: bool,
    #[serde(default)]
    message: Option<String>,
}

/// Normalize an optional target URL: blank -> None, otherwise validate it parses.
fn clean_target(raw: Option<String>) -> Result<Option<String>> {
    match raw {
        Some(s) if !s.trim().is_empty() => {
            let s = s.trim().to_string();
            url::Url::parse(&s)?;
            Ok(Some(s))
        }
        _ => Ok(None),
    }
}

/// Normalize an optional free-text message: trim, blank -> None.
fn clean_message(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/trackers", get(index))
        .route("/trackers", post(store))
        .route("/trackers/count", get(count))
        .route("/trackers/{id}", get(show))
        .route("/trackers/{id}", put(update))
        .route("/trackers/{id}", delete(destroy))
        .route("/trackers/{id}/scans", get(scans_index))
        .route("/trackers/{id}/pings", get(pings_index))
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

async fn index(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<Vec<Dto>>> {
    let (skip, take) = skippy::skip(params.skip, params.take);

    // Order by most-recently-pinged: left-join pings and sort by each tracker's
    // latest ping time. Never-pinged tags have a NULL max and sort last (MySQL
    // puts NULLs last on DESC); updated_at breaks ties within that group.
    let mut trackers = query(&params, auth.user_id)
        .join(JoinType::LeftJoin, trackers::Relation::Pings.def())
        .group_by(trackers::Column::Id)
        .order_by(Expr::cust("MAX(pings.created_at)"), Order::Desc)
        .order_by(trackers::Column::UpdatedAt, Order::Desc)
        .offset(skip)
        .limit(take)
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
        target_url: Set(clean_target(params.target_url)?),
        is_lost: Set(params.is_lost),
        message: Set(clean_message(params.message)),

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
    let mut tracker = query_one(id, auth.user_id)
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

    let tracker = query_one(id, auth.user_id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    let mut tracker = tracker.into_active_model();

    tracker.name = Set(params.name);
    tracker.desc = Set(params.desc);
    tracker.target_url = Set(clean_target(params.target_url)?);
    tracker.is_lost = Set(params.is_lost);
    tracker.message = Set(clean_message(params.message));
    tracker.save(&state.db).await?;

    Ok(Response::Accepted)
}

async fn destroy(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Response> {
    query_one(id, auth.user_id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?
        .delete(&state.db)
        .await?;

    Ok(Response::NoContent)
}

async fn scans_index(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Vec<ScanDto>>> {
    // ensure the tracker belongs to the caller before exposing its scans
    query_one(id, auth.user_id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    let scans = Scans::find()
        .filter(scans::Column::TrackerId.eq(id))
        .order_by(scans::Column::CreatedAt, sea_orm::Order::Desc)
        .limit(200)
        .into_model::<ScanDto>()
        .all(&state.db)
        .await?;

    Ok(Json(scans))
}

async fn pings_index(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Vec<PingDto>>> {
    // ensure the tracker belongs to the caller before exposing its pings
    query_one(id, auth.user_id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    let pings = Pings::find()
        .filter(pings::Column::TrackerId.eq(id))
        .order_by(pings::Column::CreatedAt, sea_orm::Order::Desc)
        .limit(200)
        .into_model::<PingDto>()
        .all(&state.db)
        .await?;

    Ok(Json(pings))
}

#[cfg(test)]
mod tests {
    use super::{clean_message, clean_target};

    #[test]
    fn clean_target_blank_becomes_none() {
        assert_eq!(clean_target(None).unwrap(), None);
        assert_eq!(clean_target(Some("".into())).unwrap(), None);
        assert_eq!(clean_target(Some("   ".into())).unwrap(), None);
    }

    #[test]
    fn clean_target_trims_and_keeps_valid_url() {
        assert_eq!(
            clean_target(Some("  https://example.com/x  ".into())).unwrap(),
            Some("https://example.com/x".to_string())
        );
    }

    #[test]
    fn clean_target_rejects_non_url() {
        assert!(clean_target(Some("not a url".into())).is_err());
    }

    #[test]
    fn clean_message_trims_and_blanks_to_none() {
        assert_eq!(clean_message(None), None);
        assert_eq!(clean_message(Some("   ".into())), None);
        assert_eq!(
            clean_message(Some("  hi there  ".into())),
            Some("hi there".to_string())
        );
    }
}
