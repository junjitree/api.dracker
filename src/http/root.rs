use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Redirect,
    routing::get,
};
use sea_orm::EntityTrait;

use crate::{AppState, Error, Result, entity::prelude::Trackers, util};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/r/{slug}", get(resolve))
}

async fn index() -> StatusCode {
    StatusCode::IM_A_TEAPOT
}

/// Resolve a tracker slug to its public ping page.
///
/// The QR tag encodes a short, stable URL (`dracker.sh/<slug>`) that hits this
/// endpoint instead of the destination directly. We can change where the tag
/// points (or revoke it) without ever reprinting the physical tag.
async fn resolve(State(state): State<AppState>, Path(slug): Path<String>) -> Result<Redirect> {
    let sqids = util::sqids()?;
    let id = *sqids.decode(&slug).first().ok_or(Error::NotFound)?;

    // 404 unknown/revoked slugs so a lost tag can be killed by deleting the row
    Trackers::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    Ok(Redirect::temporary(&format!("{}/_{}", state.spa_url, slug)))
}
