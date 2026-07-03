use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::{
    Error, Result,
    entity::{
        pings,
        prelude::{Pings, Trackers, Users},
    },
    state::AppState,
    util,
};

#[derive(Debug, Deserialize, Validate)]
struct PingParams {
    slug: String,
    #[validate(range(min = -90.0, max = 90.0))]
    lat: f64,
    #[validate(range(min = -180.0, max = 180.0))]
    lon: f64,
    #[validate(length(max = 255))]
    note: String,
}

#[derive(Debug, Deserialize, Validate)]
struct NoteParams {
    #[validate(length(min = 1, max = 255))]
    note: String,
}

/// Public, finder-facing view of a tag. Contact details and the owner's
/// message are only included while the tag is marked lost.
#[derive(Serialize)]
struct PublicDto {
    name: String,
    is_lost: bool,
    message: Option<String>,
    contact_phone: Option<String>,
    contact_address: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/ping", post(store))
        .route("/ping/{slug}", get(show))
        .route("/ping/{uuid}/note", put(set_note))
}

/// Attach a finder's note (their phone / a meetup spot) to a just-created ping.
/// Public, so it's addressed by the ping's random uuid — handed out only in the
/// response to the ping that created it — rather than the guessable sequential
/// id. It also only sets the note while it's still empty, avoiding overwrites.
async fn set_note(
    State(state): State<AppState>,
    Path(uuid): Path<Uuid>,
    Json(params): Json<NoteParams>,
) -> Result<Json<Uuid>> {
    if let Err(err) = params.validate() {
        return Err(Error::BadRequest(err.to_string()));
    }

    let ping = Pings::find()
        .filter(pings::Column::Uuid.eq(uuid))
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    if ping.note.trim().is_empty() {
        let mut ping = ping.into_active_model();
        ping.note = Set(params.note.trim().to_string());
        ping.updated_at = Set(Utc::now());
        ping.save(&state.db).await?;
    }

    Ok(Json(uuid))
}

async fn show(State(state): State<AppState>, Path(slug): Path<String>) -> Result<Json<PublicDto>> {
    let sqids = util::sqids()?;
    let id = *sqids.decode(&slug).first().ok_or(Error::NotFound)?;

    let tracker = Trackers::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    // Don't leak the owner's contact details for a tag that isn't lost.
    if !tracker.is_lost {
        return Ok(Json(PublicDto {
            name: tracker.name,
            is_lost: false,
            message: None,
            contact_phone: None,
            contact_address: None,
        }));
    }

    let owner = Users::find_by_id(tracker.user_id).one(&state.db).await?;
    let (contact_phone, contact_address) =
        owner.map(|u| (u.phone, u.address)).unwrap_or((None, None));

    Ok(Json(PublicDto {
        name: tracker.name,
        is_lost: true,
        message: tracker.message,
        contact_phone,
        contact_address,
    }))
}

/// Record a finder's location ping. Returns the ping's uuid — the only handle
/// that can attach a follow-up note (see `set_note`).
async fn store(
    State(state): State<AppState>,
    Json(params): Json<PingParams>,
) -> Result<Json<Uuid>> {
    if let Err(err) = params.validate() {
        return Err(Error::BadRequest(err.to_string()));
    }

    let sqids = util::sqids()?;
    let tracker_id = *sqids
        .decode(&params.slug)
        .first()
        .ok_or(Error::BadRequest("Invalid slug".into()))?;

    // 404 unknown/revoked slugs instead of tripping the FK into a 500
    Trackers::find_by_id(tracker_id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    let uuid = Uuid::new_v4();
    pings::ActiveModel {
        tracker_id: Set(tracker_id),
        lat: Set(params.lat),
        lon: Set(params.lon),
        note: Set(params.note),
        uuid: Set(Some(uuid.as_bytes().to_vec())),

        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    Ok(Json(uuid))
}
