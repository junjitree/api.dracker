use jsonwebtoken::{DecodingKey, EncodingKey};
use lettre::{SmtpTransport, message::Mailbox};
use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct AppState {
    pub spa_url: String,

    /// Public origin of the API (e.g. https://api.dracker.sh). Templated into
    /// the landing page so its signed-in check hits the host the session
    /// cookie lives on. Empty means same-origin — right for dev, where one
    /// process serves both the page and the API.
    pub api_url: String,

    /// Public origin of the apex domain (e.g. https://dracker.sh). Added to
    /// the CORS allowlist so the landing page's signed-in check can call the
    /// API cross-origin. Empty means the check is same-origin and CORS is
    /// not involved.
    pub apex_url: String,

    pub db: DatabaseConnection,

    pub prv_key: EncodingKey,
    pub pub_key: DecodingKey,

    pub mail: Mail,
}

#[derive(Clone)]
pub struct Mail {
    pub transport: SmtpTransport,
    pub from: Mailbox,
}
