use axum::{extract::State, Json};
use serde::Serialize;

use crate::{handlers::{AuthenticatedUser, AppError}, state::DatabaseState};

#[derive(Serialize)]
pub struct FeaturesResponse {
    pub invite_enabled: bool,
}

pub async fn get_features(
    State(db): State<DatabaseState>,
    AuthenticatedUser { .. }: AuthenticatedUser,
) -> Result<Json<FeaturesResponse>, AppError> {
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM workflows
            WHERE key = 'invite_email' AND status = 'active'
        )",
    )
    .fetch_one(&db.db_pool)
    .await?;

    Ok(Json(FeaturesResponse {
        invite_enabled: active,
    }))
}
