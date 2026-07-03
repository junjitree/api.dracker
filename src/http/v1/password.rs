use axum::{Json, Router, extract::State, routing::post};
use jsonwebtoken::{Algorithm, Header, Validation, decode, encode};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use serde::Deserialize;
use url::Url;
use validator::Validate;

use crate::{
    AppState, Error, Response, Result,
    auth::{ResetClaim, hash_password},
    entity::{prelude::Users, users},
    mail::user::send_reset,
};

#[derive(Debug, Deserialize, Validate)]
struct ResetParams {
    #[validate(length(min = 1))]
    token: String,
    #[validate(length(min = 8))]
    password: String,
    #[validate(length(min = 8))]
    password_confirm: String,
}

#[derive(Debug, Deserialize, Validate)]
struct ForgotParams {
    #[validate(email)]
    email: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/password", post(set))
        .route("/password/forgot", post(forgot))
}

async fn set(State(state): State<AppState>, Json(params): Json<ResetParams>) -> Result<Response> {
    if let Err(err) = params.validate() {
        return Err(Error::BadRequest(err.to_string()));
    }

    if params.password != params.password_confirm {
        return Err(Error::BadRequest("Passwords don't match".into()));
    }

    let validation = Validation::new(Algorithm::EdDSA);

    let claims = match decode::<ResetClaim>(&params.token, &state.pub_key, &validation) {
        Ok(data) => data,
        Err(_) => return Err(Error::InvalidCredentials),
    }
    .claims;

    let mut user = Users::find_by_id(claims.user_id)
        .filter(users::Column::Email.eq(claims.email))
        .one(&state.db)
        .await?
        .ok_or(Error::InvalidCredentials)?
        .into_active_model();

    user.password = Set(hash_password(&params.password)?);
    user.save(&state.db).await?;

    Ok(Response::Accepted)
}

async fn forgot(
    State(state): State<AppState>,
    Json(params): Json<ForgotParams>,
) -> Result<Response> {
    if let Err(err) = params.validate() {
        return Err(Error::BadRequest(err.to_string()));
    }

    let user = match Users::find()
        .filter(users::Column::Email.eq(params.email))
        .one(&state.db)
        .await?
    {
        Some(user) => user,
        None => {
            return Ok(Response::Accepted);
        }
    };

    let now = chrono::Utc::now();
    let exp = (now + chrono::Duration::days(1)).timestamp() as usize;

    let auth = ResetClaim {
        user_id: user.id,
        email: user.email.clone(),
        exp,
    };

    let token = match encode(&Header::new(Algorithm::EdDSA), &auth, &state.prv_key) {
        Ok(token) => token,
        Err(_) => return Err(Error::Internal("Could not generate reset token".into())),
    };

    let mut link = Url::parse(&state.spa_url)?;
    {
        let mut path_segments = link
            .path_segments_mut()
            .map_err(|_| url::ParseError::RelativeUrlWithoutBase)?;
        path_segments.push("password");
    }
    {
        let mut query = link.query_pairs_mut();
        query.append_pair("token", &token);
    }
    // SmtpTransport::send is blocking — keep it off the async workers
    tokio::task::spawn_blocking(move || {
        let _ = send_reset(&state.mail, &user, link.as_str());
    });

    Ok(Response::Accepted)
}
