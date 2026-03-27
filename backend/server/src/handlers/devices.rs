//! Device management handlers for listing and revoking device tokens.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::handlers::AuthenticatedUser;
use crate::AppState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Single device response item.
#[derive(Debug, Serialize)]
pub struct DeviceListResponse {
    pub id: String,
    pub device_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Response for listing devices.
#[derive(Debug, Serialize)]
pub struct ListDevicesResponse {
    pub devices: Vec<DeviceListResponse>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/user/devices
///
/// Returns a list of active (non-revoked) device tokens for the calling user.
pub async fn list_devices(
    State(state): State<AppState>,
    AuthenticatedUser { user_id }: AuthenticatedUser,
) -> Result<Json<ListDevicesResponse>, (StatusCode, Json<serde_json::Value>)> {
    let devices = state.device_repo
        .list_by_user(user_id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to list devices: {}", e);
            internal_error("Failed to list devices")
        })?;
    
    let device_responses: Vec<DeviceListResponse> = devices
        .into_iter()
        .filter(|d| d.is_valid()) // Only show valid (non-revoked, non-expired) devices
        .map(|d| DeviceListResponse {
            id: d.id.to_string(),
            device_name: d.device_name,
            created_at: d.created_at,
            last_used_at: Some(d.last_used_at),
        })
        .collect();
    
    Ok(Json(ListDevicesResponse {
        devices: device_responses,
    }))
}

/// DELETE /api/v1/user/devices/:id
///
/// Revokes a device token.
/// Users can only revoke their own devices.
pub async fn revoke_device(
    State(state): State<AppState>,
    AuthenticatedUser { user_id }: AuthenticatedUser,
    Path(device_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // First verify the device belongs to the user
    let device = state.device_repo
        .get(device_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get device: {}", e);
            internal_error("Failed to revoke device")
        })?;
    
    let device = device.ok_or_else(|| {
        not_found("Device not found")
    })?;
    
    // Verify ownership
    if device.user_id != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Cannot revoke device that does not belong to you" })),
        ));
    }
    
    // Revoke the device
    state.device_repo
        .delete(device_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to revoke device: {}", e);
            internal_error("Failed to revoke device")
        })?;
    
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn not_found(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": msg })))
}

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Test DeviceListResponse serialization
    #[test]
    fn test_device_list_response_serialization() {
        let now = chrono::Utc::now();
        let device = DeviceListResponse {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            device_name: "Test Device".to_string(),
            created_at: now,
            last_used_at: Some(now),
        };

        let json = serde_json::to_string(&device).expect("Failed to serialize");
        assert!(json.contains("Test Device"));
        assert!(json.contains("550e8400-e29b-41d4-a716-446655440000"));
    }

    /// Test ListDevicesResponse serialization
    #[test]
    fn test_list_devices_response_serialization() {
        let now = chrono::Utc::now();
        let response = ListDevicesResponse {
            devices: vec![
                DeviceListResponse {
                    id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                    device_name: "Device 1".to_string(),
                    created_at: now,
                    last_used_at: Some(now),
                },
                DeviceListResponse {
                    id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
                    device_name: "Device 2".to_string(),
                    created_at: now,
                    last_used_at: None,
                },
            ],
        };

        let json = serde_json::to_string(&response).expect("Failed to serialize");
        assert!(json.contains("Device 1"));
        assert!(json.contains("Device 2"));
        assert!(json.contains("devices"));
    }
}
