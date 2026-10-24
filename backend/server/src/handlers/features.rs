use axum::{extract::State, Json};
use serde::Serialize;

use crate::{
    handlers::{AppError, AuthenticatedUser},
    state::DatabaseState,
};
use rustshare_infrastructure::repositories::WorkflowRepository;

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
    let active = WorkflowRepository::new(db.db_pool).invite_enabled().await?;

    Ok(Json(FeaturesResponse {
        invite_enabled: active,
    }))
}

#[cfg(test)]
mod tests {
    use super::FeaturesResponse;

    #[test]
    fn response_keeps_the_public_json_shape() {
        assert_eq!(
            serde_json::to_value(FeaturesResponse {
                invite_enabled: true,
            })
            .unwrap(),
            serde_json::json!({ "invite_enabled": true })
        );
    }
}
