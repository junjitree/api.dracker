//! HTTP + DB integration tests. They exercise the real router (built by
//! `crate::build_app`) against a real MySQL/MariaDB via `oneshot`.
//!
//! They only run when `TEST_DATABASE_URL` is set (CI provides a throwaway
//! MySQL service); without it every test returns early, so a plain local
//! `cargo test` with no database still passes. Each test uses a unique email
//! and a unique `X-Forwarded-For` so they stay independent under parallelism
//! and don't trip the per-IP rate limiter.

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use jsonwebtoken::{DecodingKey, EncodingKey};
use lettre::{SmtpTransport, message::Mailbox};
use migration::{Migrator, MigratorTrait};
use sea_orm::Database;
use serde_json::{Value, json};
use tower::ServiceExt; // for `oneshot`

use crate::state::{AppState, Mail};

// Throwaway Ed25519 keypair — test-only, never used outside these tests.
const TEST_PRV_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIJMH2MByKu60/R8kjaeHkJqq95Q1IA3YGRc8AVwnBlhl\n-----END PRIVATE KEY-----\n";
const TEST_PUB_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAelvyxr9U3h2LT+Fkr7PoiYj0JolHqskqNovn4XZg9ps=\n-----END PUBLIC KEY-----\n";

/// Build the app against the test DB, or `None` when `TEST_DATABASE_URL` is
/// unset (so the suite skips cleanly on a DB-less machine).
async fn app() -> Option<Router> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let db = Database::connect(&url).await.expect("connect test db");
    Migrator::up(&db, None).await.expect("run migrations");

    let state = AppState {
        db,
        mail: Mail {
            transport: SmtpTransport::unencrypted_localhost(),
            from: Mailbox::new(Some("Test".into()), "test@localhost".parse().unwrap()),
        },
        prv_key: EncodingKey::from_ed_pem(TEST_PRV_PEM.as_bytes()).unwrap(),
        pub_key: DecodingKey::from_ed_pem(TEST_PUB_PEM.as_bytes()).unwrap(),
        spa_url: "http://localhost".into(),
    };
    Some(crate::build_app(state))
}

/// A request builder that stamps the caller's fake client IP (for the rate
/// limiter's key extractor) and an optional Cookie header.
struct Req {
    method: &'static str,
    uri: String,
    ip: String,
    cookie: Option<String>,
    csrf: Option<String>,
    body: Value,
}

impl Req {
    fn new(method: &'static str, uri: &str, ip: &str) -> Self {
        Self {
            method,
            uri: uri.into(),
            ip: ip.into(),
            cookie: None,
            csrf: None,
            body: Value::Null,
        }
    }
    fn cookie(mut self, c: String) -> Self {
        self.cookie = Some(c);
        self
    }
    fn csrf(mut self, c: String) -> Self {
        self.csrf = Some(c);
        self
    }
    fn json(mut self, body: Value) -> Self {
        self.body = body;
        self
    }

    fn build(self) -> Request<Body> {
        let mut b = Request::builder()
            .method(self.method)
            .uri(self.uri)
            .header("x-forwarded-for", self.ip)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(c) = self.cookie {
            b = b.header(header::COOKIE, c);
        }
        if let Some(c) = self.csrf {
            b = b.header("x-csrf-token", c);
        }
        let body = if self.body.is_null() {
            Body::empty()
        } else {
            Body::from(self.body.to_string())
        };
        b.body(body).unwrap()
    }
}

/// Send a request against a fresh clone of the app; return status + headers +
/// parsed JSON body (Null if the body isn't JSON).
async fn send(app: &Router, req: Req) -> (StatusCode, HeaderMap, Value) {
    let res = app.clone().oneshot(req.build()).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, value)
}

/// Pull a `name=value` out of a response's Set-Cookie headers.
fn set_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all(header::SET_COOKIE).iter().find_map(|hv| {
        let s = hv.to_str().ok()?;
        let first = s.split(';').next()?;
        let (k, v) = first.split_once('=')?;
        (k.trim() == name).then(|| v.trim().to_string())
    })
}

/// Sign up + log in a fresh user, returning (auth cookie value, csrf token).
async fn login(app: &Router, ip: &str, email: &str) -> (String, String) {
    let body =
        json!({"email": email, "password": "password123", "given_name": "T", "surname": "U"});
    let (s, _, _) = send(app, Req::new("POST", "/v1/signup", ip).json(body)).await;
    assert_eq!(s, StatusCode::CREATED, "signup");

    let creds = json!({"email": email, "password": "password123"});
    let (s, h, _) = send(app, Req::new("POST", "/v1/login/cookie", ip).json(creds)).await;
    assert_eq!(s, StatusCode::CREATED, "login");
    let auth = set_cookie(&h, "authorization").expect("auth cookie");

    let (s, h, _) = send(app, Req::new("GET", "/v1/csrf", ip)).await;
    assert_eq!(s, StatusCode::CREATED, "csrf");
    let csrf = h.get("x-csrf-token").unwrap().to_str().unwrap().to_string();

    (format!("authorization={auth}; x-csrf-token={csrf}"), csrf)
}

#[tokio::test]
async fn signup_login_and_wrong_password() {
    let Some(app) = app().await else { return };
    let ip = "10.10.0.1";
    let email = "signup_login@test.local";

    let body =
        json!({"email": email, "password": "password123", "given_name": "T", "surname": "U"});
    let (s, _, _) = send(&app, Req::new("POST", "/v1/signup", ip).json(body)).await;
    assert_eq!(s, StatusCode::CREATED);

    let (s, h, _) = send(
        &app,
        Req::new("POST", "/v1/login/cookie", ip)
            .json(json!({"email": email, "password": "password123"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert!(set_cookie(&h, "authorization").is_some());

    let (s, _, _) = send(
        &app,
        Req::new("POST", "/v1/login/cookie", ip).json(json!({"email": email, "password": "nope"})),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn csrf_is_enforced_on_mutations() {
    let Some(app) = app().await else { return };
    let ip = "10.10.0.2";
    let (cookie, csrf) = login(&app, ip, "csrf_enforced@test.local").await;

    let payload = json!({"email": "csrf_enforced@test.local", "given_name": "T", "surname": "U"});

    // authenticated but no CSRF header -> rejected
    let (s, _, _) = send(
        &app,
        Req::new("PUT", "/v1/users/me", ip)
            .cookie(cookie.clone())
            .json(payload.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // with the matching CSRF header -> accepted
    let (s, _, _) = send(
        &app,
        Req::new("PUT", "/v1/users/me", ip)
            .cookie(cookie)
            .csrf(csrf)
            .json(payload),
    )
    .await;
    assert_eq!(s, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn ping_location_with_note() {
    let Some(app) = app().await else { return };
    let ip = "10.10.0.3";
    let (cookie, csrf) = login(&app, ip, "ping_flow@test.local").await;

    // create a tracker, derive its public slug
    let (s, _, v) = send(
        &app,
        Req::new("POST", "/v1/trackers", ip)
            .cookie(cookie.clone())
            .csrf(csrf)
            .json(json!({"name": "Keys", "desc": ""})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let id = v.as_u64().expect("tracker id");
    let slug = crate::util::sqids().unwrap().encode(&[id]).unwrap();

    // a finder pings a location AND a note together, in one request
    let (s, _, _) = send(
        &app,
        Req::new("POST", "/v1/ping", ip)
            .json(json!({"slug": slug, "lat": 14.6, "lon": 121.0, "note": "found it, call me"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // the owner sees the ping with both coordinates and the note
    let (s, _, v) = send(
        &app,
        Req::new("GET", &format!("/v1/trackers/{id}/pings"), ip).cookie(cookie.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let pings = v.as_array().expect("pings array");
    assert_eq!(pings.len(), 1);
    assert_eq!(pings[0]["note"], "found it, call me");
    assert_eq!(pings[0]["lat"].as_f64(), Some(14.6));

    // ...and the tracker detail exposes the ping as its last activity
    let (s, _, v) = send(
        &app,
        Req::new("GET", &format!("/v1/trackers/{id}"), ip).cookie(cookie),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["last_ping_at"].is_string(), "last_ping_at should be set");
}

#[tokio::test]
async fn ping_note_only_flow() {
    let Some(app) = app().await else { return };
    let ip = "10.10.0.7";
    let (cookie, csrf) = login(&app, ip, "note_only@test.local").await;

    let (s, _, v) = send(
        &app,
        Req::new("POST", "/v1/trackers", ip)
            .cookie(cookie.clone())
            .csrf(csrf)
            .json(json!({"name": "Umbrella", "desc": ""})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let id = v.as_u64().expect("tracker id");
    let slug = crate::util::sqids().unwrap().encode(&[id]).unwrap();

    // a note without a location is accepted
    let (s, _, _) = send(
        &app,
        Req::new("POST", "/v1/ping", ip)
            .json(json!({"slug": slug, "note": "left it with the barista"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // ...but an empty ping (no coords, no note) is not
    let (s, _, _) = send(
        &app,
        Req::new("POST", "/v1/ping", ip).json(json!({"slug": slug, "note": "  "})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // ...nor is half a coordinate pair
    let (s, _, _) = send(
        &app,
        Req::new("POST", "/v1/ping", ip).json(json!({"slug": slug, "lat": 1.0, "note": "x"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // the owner sees the note-only ping with null coordinates
    let (s, _, v) = send(
        &app,
        Req::new("GET", &format!("/v1/trackers/{id}/pings"), ip).cookie(cookie),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let pings = v.as_array().expect("pings array");
    assert_eq!(pings.len(), 1);
    assert_eq!(pings[0]["note"], "left it with the barista");
    assert!(pings[0]["lat"].is_null());
    assert!(pings[0]["lon"].is_null());
}

#[tokio::test]
async fn trackers_list_and_detail() {
    let Some(app) = app().await else { return };
    let ip = "10.10.0.5";
    let (cookie, csrf) = login(&app, ip, "trackers_list@test.local").await;

    let (s, _, v) = send(
        &app,
        Req::new("POST", "/v1/trackers", ip)
            .cookie(cookie.clone())
            .csrf(csrf)
            .json(json!({"name": "Camera bag", "desc": "black"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let id = v.as_u64().expect("tracker id");

    // The list query left-joins pings and orders by MAX(pings.created_at) with
    // a GROUP BY — the app's most DB-specific query. Exercise it end to end.
    let (s, _, v) = send(
        &app,
        Req::new("GET", "/v1/trackers", ip).cookie(cookie.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let list = v.as_array().expect("tracker array");
    let mine = list
        .iter()
        .find(|t| t["id"].as_u64() == Some(id))
        .expect("created tracker present in list");
    assert_eq!(mine["name"], "Camera bag");
    assert!(mine["slug"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(mine["last_ping_at"].is_null(), "never pinged => null");

    let (s, _, v) = send(
        &app,
        Req::new("GET", &format!("/v1/trackers/{id}"), ip).cookie(cookie),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["id"].as_u64(), Some(id));
}

#[tokio::test]
async fn scan_recorded_and_listed() {
    let Some(app) = app().await else { return };
    let ip = "10.10.0.6";
    let (cookie, csrf) = login(&app, ip, "scan_flow@test.local").await;

    // create a tracker, derive its public slug
    let (s, _, v) = send(
        &app,
        Req::new("POST", "/v1/trackers", ip)
            .cookie(cookie.clone())
            .csrf(csrf)
            .json(json!({"name": "Wallet", "desc": ""})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let id = v.as_u64().expect("tracker id");
    let slug = crate::util::sqids().unwrap().encode(&[id]).unwrap();

    // hitting the public QR redirect records a scan
    let (s, _, _) = send(&app, Req::new("GET", &format!("/r/{slug}"), ip)).await;
    assert_eq!(s, StatusCode::TEMPORARY_REDIRECT);

    // The insert is spawned off the request path (best-effort, errors
    // swallowed), so poll for it to land. This is the only coverage of the
    // scans table — which historically existed in prod but not in migrations —
    // so a missing/mismatched table surfaces here as a 500 or an empty list.
    let mut scans = Vec::new();
    for _ in 0..20 {
        let (s, _, v) = send(
            &app,
            Req::new("GET", &format!("/v1/trackers/{id}/scans"), ip).cookie(cookie.clone()),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        scans = v.as_array().cloned().unwrap_or_default();
        if !scans.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(scans.len(), 1, "scan row recorded");
}

#[tokio::test]
async fn ping_rejects_bad_input() {
    let Some(app) = app().await else { return };
    let ip = "10.10.0.4";

    // out-of-range latitude -> 400
    let (s, _, _) = send(
        &app,
        Req::new("POST", "/v1/ping", ip)
            .json(json!({"slug": "abc", "lat": 999.0, "lon": 0.0, "note": ""})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // decodable but nonexistent slug -> 404 (not a 500 from the FK)
    let ghost = crate::util::sqids().unwrap().encode(&[9_999_999]).unwrap();
    let (s, _, _) = send(
        &app,
        Req::new("POST", "/v1/ping", ip)
            .json(json!({"slug": ghost, "lat": 1.0, "lon": 1.0, "note": ""})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
