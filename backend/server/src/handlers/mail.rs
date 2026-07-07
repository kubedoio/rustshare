use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

use crate::state::AppState;

pub async fn list_mail_messages(
    State(_state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(json!({ "messages": [] })))
}
