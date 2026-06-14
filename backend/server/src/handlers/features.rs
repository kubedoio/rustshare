use axum::{extract::State, Json};
use serde::Serialize;

use crate::{
    handlers::{AppError, AuthenticatedUser},
    state::DatabaseState,
};

#[derive(Serialize, utoipa::ToSchema)]
pub struct FeaturesResponse {
    pub invite_enabled: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/features",
    tag = "Features",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
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
