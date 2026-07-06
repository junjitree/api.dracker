use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, header::USER_AGENT},
    response::{Html, Redirect},
    routing::get,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};

use crate::{AppState, Error, Result, entity::prelude::Trackers, entity::scans, util};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/r/{slug}", get(resolve))
}

/// The marketing landing page, served on the apex domain (dracker.sh).
///
/// Embedded in the binary so the deploy stays a single artifact; app links are
/// templated so dev environments point at their local SPA.
const LANDING: &str = include_str!("../../static/index.html");

async fn index(State(state): State<AppState>) -> Html<String> {
    Html(LANDING.replace("{{spa_url}}", &state.spa_url))
}

/// Resolve a tracker slug to its destination.
///
/// The QR tag encodes a short, stable URL (`dracker.sh/<slug>`) that hits this
/// endpoint instead of the destination directly. This lets us repoint or revoke
/// a tag without reprinting it, and record a scan event each time it's hit.
async fn resolve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Redirect> {
    let sqids = util::sqids()?;
    let id = *sqids.decode(&slug).first().ok_or(Error::NotFound)?;

    // 404 unknown/revoked slugs so a lost tag can be killed by deleting the row
    let tracker = Trackers::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    // record the scan (best-effort: never block the redirect on logging)
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string());
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(512).collect::<String>());
    let db = state.db.clone();
    tokio::spawn(async move {
        let _ = scans::ActiveModel {
            tracker_id: Set(id),
            ip: Set(ip),
            user_agent: Set(user_agent),
            ..Default::default()
        }
        .insert(&db)
        .await;
    });

    // A lost tag always lands on the public page so the finder sees the
    // owner's contact info; otherwise honor a custom destination if set.
    let target = if tracker.is_lost {
        format!("{}/_{}", state.spa_url, slug)
    } else {
        tracker
            .target_url
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("{}/_{}", state.spa_url, slug))
    };

    Ok(Redirect::temporary(&target))
}
