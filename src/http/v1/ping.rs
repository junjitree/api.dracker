use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::{
    Error, Result,
    entity::{
        pings,
        prelude::{Trackers, Users},
    },
    mail::user::send_ping_notification,
    state::AppState,
    util,
};

#[derive(Debug, Deserialize, Validate)]
struct PingParams {
    slug: String,
    // Optional as a pair: a finder may leave only a note instead of a location.
    #[validate(range(min = -90.0, max = 90.0))]
    #[serde(default)]
    lat: Option<f64>,
    #[validate(range(min = -180.0, max = 180.0))]
    #[serde(default)]
    lon: Option<f64>,
    #[validate(length(max = 255))]
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

/// Record a finder's ping — a location, a note, or both — and email the owner.
async fn store(
    State(state): State<AppState>,
    Json(params): Json<PingParams>,
) -> Result<Json<Uuid>> {
    if let Err(err) = params.validate() {
        return Err(Error::BadRequest(err.to_string()));
    }

    // Coordinates come as a pair, and an empty ping (no location, no note)
    // carries nothing for the owner.
    let coords = match (params.lat, params.lon) {
        (Some(lat), Some(lon)) => Some((lat, lon)),
        (None, None) => None,
        _ => return Err(Error::BadRequest("lat and lon go together".into())),
    };
    if coords.is_none() && params.note.trim().is_empty() {
        return Err(Error::BadRequest("Share a location or leave a note".into()));
    }

    let sqids = util::sqids()?;
    let tracker_id = *sqids
        .decode(&params.slug)
        .first()
        .ok_or(Error::BadRequest("Invalid slug".into()))?;

    // 404 unknown/revoked slugs instead of tripping the FK into a 500
    let tracker = Trackers::find_by_id(tracker_id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    let uuid = Uuid::new_v4();
    pings::ActiveModel {
        tracker_id: Set(tracker_id),
        lat: Set(params.lat),
        lon: Set(params.lon),
        note: Set(params.note.trim().to_string()),
        uuid: Set(Some(uuid.as_bytes().to_vec())),

        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    // Best-effort: let the owner know their tag was pinged. Off the request
    // path (blocking SMTP), never fails the ping.
    if let Some(owner) = Users::find_by_id(tracker.user_id).one(&state.db).await? {
        let mail = state.mail.clone();
        let spa_url = state.spa_url.clone();
        let note = params.note.trim().to_string();
        let tag = tracker.name.clone();
        let tid = tracker.id;
        tokio::task::spawn_blocking(move || {
            if let Err(err) =
                send_ping_notification(&mail, &owner, &tag, tid, coords, &note, &spa_url)
            {
                tracing::error!("ping notification email failed: {err}");
            }
        });
    }

    Ok(Json(uuid))
}

#[cfg(test)]
mod tests {
    use super::PingParams;
    use validator::Validate;

    fn ping(lat: f64, lon: f64, note: &str) -> PingParams {
        PingParams {
            slug: "abc".into(),
            lat: Some(lat),
            lon: Some(lon),
            note: note.into(),
        }
    }

    #[test]
    fn ping_accepts_in_range_coords() {
        assert!(ping(14.6, 120.98, "").validate().is_ok());
        assert!(ping(-90.0, 180.0, "meet at cafe").validate().is_ok());
    }

    #[test]
    fn ping_rejects_out_of_range_coords() {
        assert!(ping(90.1, 0.0, "").validate().is_err());
        assert!(ping(0.0, 200.0, "").validate().is_err());
    }

    #[test]
    fn ping_rejects_overlong_note() {
        assert!(ping(0.0, 0.0, &"x".repeat(256)).validate().is_err());
        assert!(ping(0.0, 0.0, &"x".repeat(255)).validate().is_ok());
    }
}
