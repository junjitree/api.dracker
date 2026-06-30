use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel};
use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    entity::{
        pings,
        prelude::{Pings, Trackers, Users},
    },
    state::AppState,
    util,
};

#[derive(Debug, Deserialize)]
struct PingParams {
    slug: String,
    lat: f64,
    lon: f64,
    note: String,
}

#[derive(Debug, Deserialize)]
struct NoteParams {
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
        .route("/ping/{id}/note", put(set_note))
}

/// Attach a finder's note (their phone / a meetup spot) to a just-created ping.
/// Public and id-addressed, so it only sets the note while it's still empty —
/// avoiding overwrites of an existing note.
async fn set_note(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(params): Json<NoteParams>,
) -> Result<Json<u64>> {
    let ping = Pings::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    if ping.note.trim().is_empty() {
        let mut ping = ping.into_active_model();
        ping.note = Set(params.note.trim().to_string());
        ping.updated_at = Set(Utc::now());
        ping.save(&state.db).await?;
    }

    Ok(Json(id))
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

async fn store(State(state): State<AppState>, Json(params): Json<PingParams>) -> Result<Json<u64>> {
    let sqids = util::sqids()?;
    let tracker_id = sqids.decode(&params.slug);
    if tracker_id.is_empty() {
        return Err(Error::BadRequest("Invalid tracker_id".into()));
    }

    let tracker_id = tracker_id[0];
    let ping = pings::ActiveModel {
        tracker_id: Set(tracker_id),
        lat: Set(params.lat),
        lon: Set(params.lon),
        note: Set(params.note),

        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    Ok(Json(ping.id))
}
