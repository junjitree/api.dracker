use axum::{
    Extension, Json, Router,
    extract::State,
    routing::{delete, get, put},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, FromQueryResult, IntoActiveModel,
    ModelTrait, QueryFilter, QuerySelect, prelude::DateTimeUtc,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::{
    AppState, Error, Response, Result,
    auth::AuthClaim,
    entity::{prelude::Users, users},
};

#[derive(Debug, Deserialize, Validate)]
pub struct Params {
    #[validate(email)]
    pub email: String,
    pub given_name: String,
    pub surname: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
}

#[derive(Serialize, FromQueryResult)]
pub struct Dto {
    pub id: u64,
    pub email: String,
    pub given_name: String,
    pub surname: String,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users/me", get(me))
        .route("/users/me", put(update))
        .route("/users/me", delete(destroy))
}

async fn me(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthClaim>,
) -> Result<Json<Dto>> {
    let user = Users::find_by_id(auth.user_id)
        .select_only()
        .column(users::Column::Id)
        .column(users::Column::Email)
        .column(users::Column::GivenName)
        .column(users::Column::Surname)
        .column(users::Column::Phone)
        .column(users::Column::Address)
        .column(users::Column::CreatedAt)
        .column(users::Column::UpdatedAt)
        .into_model::<Dto>()
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    Ok(Json(user))
}

async fn update(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
    Json(params): Json<Params>,
) -> Result<Response> {
    let user = Users::find_by_id(auth.user_id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    if user.email != params.email {
        let existing_user = Users::find()
            .filter(users::Column::Email.eq(&params.email))
            .one(&state.db)
            .await?;

        if existing_user.is_some() {
            return Err(Error::BadRequest("Email is taken".to_string()));
        }
    }

    let mut user = user.into_active_model();
    user.email = Set(params.email);
    user.given_name = Set(params.given_name);
    user.surname = Set(params.surname);
    user.phone = Set(params.phone);
    user.address = Set(params.address);
    user.updated_at = Set(Utc::now());
    user.save(&state.db).await?;

    Ok(Response::Accepted)
}

async fn destroy(
    Extension(auth): Extension<AuthClaim>,
    State(state): State<AppState>,
) -> Result<Response> {
    Users::find_by_id(auth.user_id)
        .one(&state.db)
        .await?
        .ok_or(Error::NotFound)?
        .delete(&state.db)
        .await?;

    Ok(Response::NoContent)
}
