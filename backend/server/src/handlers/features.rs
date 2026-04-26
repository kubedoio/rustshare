use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::{handlers::AuthenticatedUser, state::DatabaseState};

#[derive(Serialize)]
pub struct FeaturesResponse {
    pub invite_enabled: bool,
}

pub async fn get_features(
    State(db): State<DatabaseState>,
    AuthenticatedUser { .. }: AuthenticatedUser,
) -> Result<Json<FeaturesResponse>, (StatusCode, Json<crate::handlers::ErrorResponse>)> {
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM workflows
            WHERE key = 'invite_email' AND status = 'active'
        )",
    )
    .fetch_one(&db.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::handlers::ErrorResponse::new(e.to_string())),
        )
    })?;

    Ok(Json(FeaturesResponse {
        invite_enabled: active,
    }))
}
