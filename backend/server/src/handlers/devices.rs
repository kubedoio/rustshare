//! Device management handlers for listing and revoking device tokens.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use rustshare_infrastructure::repositories::DeviceRecord;
use serde::Serialize;
use uuid::Uuid;

use crate::handlers::AppError;
use crate::handlers::AuthenticatedUser;
use crate::state::DatabaseState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Single device response item.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeviceListResponse {
    pub id: String,
    pub device_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Response for listing devices.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListDevicesResponse {
    pub devices: Vec<DeviceListResponse>,
}

// ---------------------------------------------------------------------------
// Response mapping
// ---------------------------------------------------------------------------

impl From<DeviceRecord> for DeviceListResponse {
    fn from(row: DeviceRecord) -> Self {
        DeviceListResponse {
            id: row.id.to_string(),
            device_name: row.device_name,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/user/devices
///
/// Returns a list of active (non-revoked) device tokens for the calling user.
#[utoipa::path(
    get,
    path = "/api/v1/user/devices",
    tag = "Devices",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_devices(
    State(db): State<DatabaseState>,
    AuthenticatedUser { tenant_id, user_id }: AuthenticatedUser,
) -> Result<Json<ListDevicesResponse>, AppError> {
    let rows = db
        .device_repository
        .list_for_user(tenant_id, user_id)
        .await?;

    Ok(Json(ListDevicesResponse {
        devices: rows.into_iter().map(DeviceListResponse::from).collect(),
    }))
}

/// DELETE /api/v1/user/devices/:id
///
/// Revokes a device token (sets revoked_at = NOW()).
/// Users can only revoke their own devices.
/// Returns 404 if the device doesn't belong to the user.
#[utoipa::path(
    delete,
    operation_id = "revoke_user_device",
    path = "/api/v1/user/devices/{id}",
    tag = "Devices",
    params(("id" = Uuid, Path, description = "Device token Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn revoke_device(
    State(db): State<DatabaseState>,
    AuthenticatedUser { tenant_id, user_id }: AuthenticatedUser,
    Path(device_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    if !db
        .device_repository
        .revoke_for_user(tenant_id, user_id, device_id)
        .await?
    {
        return Err(AppError::not_found("Device not found"));
    }

    Ok(StatusCode::NO_CONTENT)
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

    /// Test DeviceRow to DeviceListResponse conversion
    #[test]
    fn test_device_record_conversion() {
        let now = chrono::Utc::now();
        let row = DeviceRecord {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            device_name: "My Device".to_string(),
            created_at: now,
            last_used_at: Some(now),
        };

        let response: DeviceListResponse = row.into();
        assert_eq!(response.id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(response.device_name, "My Device");
        assert_eq!(response.created_at, now);
        assert_eq!(response.last_used_at, Some(now));
    }
}
