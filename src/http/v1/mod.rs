use std::sync::Arc;
use std::time::Duration;

use axum::{Router, middleware};
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};

use crate::{AppState, http::middleware::auth};

pub mod auth;
pub mod password;
pub mod ping;
pub mod pings;
pub mod signup;
pub mod tokens;
pub mod trackers;
pub mod users;

pub fn routes(state: &AppState) -> Router<AppState> {
    // Per-IP rate limit on the public (unauthenticated) surface — login
    // brute-force, signup/forgot email spam, and ping floods all live here.
    // Behind nginx, so the real client IP comes from X-Forwarded-For, which
    // SmartIpKeyExtractor reads (falling back to the socket address).
    let governor = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2) // replenish one request every 2s...
            .burst_size(30) // ...after an initial burst (app load, finder flow)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .unwrap(),
    );
    // Governor keeps per-IP state in a map; sweep idle entries periodically so
    // it doesn't grow unbounded.
    let limiter = governor.limiter().clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            limiter.retain_recent();
        }
    });

    // INFO: PUBLIC ROUTES
    let publ_router = Router::new()
        .merge(auth::routes(state))
        .merge(password::routes())
        .merge(ping::routes())
        .merge(signup::routes())
        .layer(GovernorLayer::new(governor));

    // WARN: AUTHENTICATED ROUTES
    let auth_router = Router::new()
        .merge(pings::routes())
        .merge(tokens::routes())
        .merge(trackers::routes())
        .merge(users::routes())
        .layer(middleware::from_fn_with_state(state.clone(), auth));

    Router::new().nest("/v1", Router::new().merge(publ_router).merge(auth_router))
}
