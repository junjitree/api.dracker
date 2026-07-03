use std::time::Duration;

use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tracing::{error, info};

use crate::{
    entity::{prelude::*, scans, user_tokens},
    http::v1::auth::SESSION_DAYS,
    state::AppState,
};

/// How long a raw scan (IP + user-agent) is kept before it's pruned. Scans are
/// a soft "how often was this tag hit" signal, not something to retain forever.
const SCAN_RETENTION_DAYS: i64 = 90;

/// Start the background maintenance loop: prune stale rows now, then daily.
pub fn start(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(24 * 60 * 60));
        loop {
            interval.tick().await;
            prune(&state).await;
        }
    });
}

async fn prune(state: &AppState) {
    // Old scans: drop the retained IP/user-agent once they're past the window.
    let scan_cutoff = Utc::now() - chrono::Duration::days(SCAN_RETENTION_DAYS);
    match Scans::delete_many()
        .filter(scans::Column::CreatedAt.lt(scan_cutoff))
        .exec(&state.db)
        .await
    {
        Ok(res) if res.rows_affected > 0 => info!("cron: pruned {} old scans", res.rows_affected),
        Ok(_) => {}
        Err(err) => error!("cron: scan prune failed: {err}"),
    }

    // Expired sessions: the JWT is valid for SESSION_DAYS from last use, so a
    // token whose row hasn't been touched in that long can never authenticate
    // again — clear the dead rows.
    let token_cutoff = Utc::now() - chrono::Duration::days(SESSION_DAYS);
    match UserTokens::delete_many()
        .filter(user_tokens::Column::UpdatedAt.lt(token_cutoff))
        .exec(&state.db)
        .await
    {
        Ok(res) if res.rows_affected > 0 => {
            info!("cron: pruned {} expired sessions", res.rows_affected)
        }
        Ok(_) => {}
        Err(err) => error!("cron: token prune failed: {err}"),
    }
}
