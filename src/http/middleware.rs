use axum::{
    extract::{Request, State},
    http::{Method, header},
    middleware::Next,
    response::IntoResponse,
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, Validation, decode};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};

use crate::{
    AppState, Error, auth::AuthClaim, entity::prelude::*, entity::user_tokens,
    http::v1::auth::X_CSRF_TOKEN,
};

pub async fn auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<impl IntoResponse, Error> {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header_str| header_str.strip_prefix("Bearer "));

    let token = match token {
        Some(token) => Some(token.to_string()),
        None => CookieJar::from_headers(request.headers())
            .get(header::AUTHORIZATION.as_str())
            .map(|cookie| cookie.value().to_string()),
    };

    let token = token.ok_or(Error::Unauthorized)?;
    let validation = Validation::new(Algorithm::EdDSA);
    let token_data = match decode::<AuthClaim>(&token, &state.pub_key, &validation) {
        Ok(data) => data,
        Err(_) => return Err(Error::Unauthorized),
    };

    let user_token = UserTokens::find()
        .filter(user_tokens::Column::Token.eq(token_data.claims.uuid))
        .one(&state.db)
        .await?
        .ok_or(Error::Unauthorized)?;

    // "Last used" only needs minute resolution — skip the write (one per
    // authenticated request otherwise) while the timestamp is fresh.
    if Utc::now() - user_token.updated_at > Duration::seconds(60) {
        let mut user_token = user_token.into_active_model();
        user_token.updated_at = Set(Utc::now());
        user_token.save(&state.db).await?;
    }

    request.extensions_mut().insert(token_data.claims);

    if request.method() == Method::GET {
        return Ok(next.run(request).await);
    }

    let csrf_header = request
        .headers()
        .get(X_CSRF_TOKEN)
        .and_then(|header| header.to_str().ok())
        .map(String::from)
        .ok_or(Error::Forbidden)?;

    let csrf_cookie = CookieJar::from_headers(request.headers())
        .get(X_CSRF_TOKEN)
        .map(|cookie| cookie.value().to_string())
        .ok_or(Error::Forbidden)?;

    if csrf_header != csrf_cookie {
        return Err(Error::Forbidden);
    }

    Ok(next.run(request).await)
}
