use axum::Router;
use axum::http::HeaderName;
use axum::http::Method;
use axum::http::header;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
use lettre::SmtpTransport;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::Tls;
use lettre::transport::smtp::client::TlsParameters;
use migration::{Migrator, MigratorTrait};
use sea_orm::Database;
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tracing::info;

mod auth;
mod cron;
mod crypto;
mod entity;
mod error;
mod http;
mod mail;
mod response;
mod result;
mod skippy;
mod state;
mod util;

use crate::http::{DEFAULT_PORT, v1::auth::X_CSRF_TOKEN};
use crate::state::AppState;
use crate::state::Mail;

pub use self::error::*;
pub use self::response::*;
pub use self::result::*;

#[tokio::main]
async fn main() -> Result<()> {
    util::init();

    let app_port: u16 = env::var("APP_PORT")
        .map(|s| s.parse::<u16>())
        .unwrap_or(Ok(DEFAULT_PORT))
        .unwrap_or(DEFAULT_PORT);

    let prv_pem = fs::read(".auth.key.private.pem")
        .expect("Missing private key. Have you run the './bin/key' script?");
    let pub_pem = fs::read(".auth.key.public.pem")
        .expect("Missing public key. Have you run the './bin/key' script?");

    let prv_key = EncodingKey::from_ed_pem(&prv_pem).unwrap();
    let pub_key = DecodingKey::from_ed_pem(&pub_pem).unwrap();

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = Database::connect(db_url).await?;

    // The deploy script only rebuilds + restarts; schema changes ship by
    // running any pending migrations here on boot.
    Migrator::up(&db, None)
        .await
        .expect("Could not run database migrations");

    let spa_url = env::var("SPA_URL").expect("SPA_URL must be set");

    let mail_host = env::var("MAIL_HOST").expect("MAIL_HOST must be set");
    let mail_user = env::var("MAIL_USER").expect("MAIL_USER must be set");
    let mail_pass = env::var("MAIL_PASS").expect("MAIL_PASS must be set");
    let mail_name = env::var("MAIL_NAME").expect("MAIL_NAME must be set");
    let mail_addr = env::var("MAIL_ADDR").expect("MAIL_ADDR must be set");
    let mail_port: u16 = env::var("MAIL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(587);

    let creds = Credentials::new(mail_user.clone(), mail_pass);

    let tls = TlsParameters::new(mail_host.clone()).expect("Could not configure TLS");
    // 465/2465 speak implicit TLS (SMTPS, TLS from the first byte); 587/2587/25
    // speak STARTTLS (plaintext greeting, then upgrade). Picking the wrong one
    // makes the SMTP conversation hang until it times out.
    let transport_builder = if matches!(mail_port, 465 | 2465) {
        SmtpTransport::relay(&mail_host)
            .unwrap()
            .tls(Tls::Wrapper(tls))
    } else {
        SmtpTransport::starttls_relay(&mail_host)
            .unwrap()
            .tls(Tls::Required(tls))
    };
    let transport = transport_builder.port(mail_port).credentials(creds).build();

    let from = Mailbox::new(Some(mail_name), mail_addr.parse().unwrap());

    let mail = Mail { transport, from };

    let state = AppState {
        db,
        mail,
        prv_key,
        pub_key,
        spa_url,
    };

    // Background maintenance: prune old scans + expired session rows daily.
    cron::start(state.clone());

    let app = build_app(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], app_port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on http://{}", listener.local_addr().unwrap());
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

/// Assemble the full application router (routes + CORS + compression) from a
/// built `AppState`. Split out from `main` so integration tests can build the
/// exact same app and drive it with `tower::ServiceExt::oneshot`.
fn build_app(state: AppState) -> Router {
    let mut origins = vec![state.spa_url.parse().unwrap()];
    if cfg!(debug_assertions) {
        // INFO: This is for local development
        origins.push("http://localhost:42069".parse().unwrap());
    }

    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([
            header::AUTHORIZATION,
            header::ACCEPT,
            header::CONTENT_TYPE,
            HeaderName::from_static(X_CSRF_TOKEN),
        ])
        .expose_headers([HeaderName::from_static(X_CSRF_TOKEN)])
        .max_age(Duration::from_secs(3600))
        .allow_credentials(true);

    Router::new()
        .merge(http::root::routes())
        .merge(http::v1::routes(&state))
        .with_state(state)
        .layer(cors)
        .layer(CompressionLayer::new())
}

#[cfg(test)]
mod integration;
