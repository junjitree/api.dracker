use axum::{Json, Router, extract::State, routing::post};
use jsonwebtoken::{Algorithm, Header, encode};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use url::Url;
use validator::Validate;

use crate::{
    AppState, Error, Response, Result,
    auth::{ResetClaim, hash_password, password_fingerprint},
    entity::{prelude::Users, users},
    mail::user::send_welcome,
};

#[derive(Debug, Deserialize, Validate)]
pub struct Params {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
    pub given_name: String,
    pub surname: String,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/signup", post(store))
}

async fn store(State(state): State<AppState>, Json(params): Json<Params>) -> Result<Response> {
    if let Err(err) = params.validate() {
        return Err(Error::BadRequest(err.to_string()));
    }

    let existing_user = Users::find()
        .filter(users::Column::Email.eq(&params.email))
        .one(&state.db)
        .await?;

    if existing_user.is_some() {
        return Err(Error::BadRequest("Email is taken".to_string()));
    }

    let user = users::ActiveModel {
        email: Set(params.email),
        password: Set(hash_password(&params.password)?),
        given_name: Set(params.given_name),
        surname: Set(params.surname),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    let now = chrono::Utc::now();
    let exp = (now + chrono::Duration::days(1)).timestamp() as usize;

    let auth = ResetClaim {
        user_id: user.id,
        email: user.email.clone(),
        pw: password_fingerprint(&user.password),
        exp,
    };

    let token = match encode(&Header::new(Algorithm::EdDSA), &auth, &state.prv_key) {
        Ok(token) => token,
        Err(_) => return Err(Error::Internal("Could not generate reset token".into())),
    };

    let user_id = user.id;
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
        if let Err(err) = send_welcome(&state.mail, &user, link.as_str()) {
            tracing::error!("send_welcome email failed: {err}");
        }
    });

    Ok(Response::Created(user_id))
}
