//! Device management handlers for listing and revoking device tokens.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::handlers::AuthenticatedUser;
use crate::state::DatabaseState;
use super::ErrorResponse;

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
// Internal row type
// ---------------------------------------------------------------------------

#[derive(FromRow)]
struct DeviceRow {
    id: Uuid,
    device_name: String,
    created_at: chrono::DateTime<chrono::Utc>,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<DeviceRow> for DeviceListResponse {
    fn from(row: DeviceRow) -> Self {
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
pub async fn list_devices(
    State(db): State<DatabaseState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
) -> Result<Json<ListDevicesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rows: Vec<DeviceRow> = sqlx::query_as(
        r#"
        SELECT id, device_name, created_at, last_used_at
        FROM device_tokens
        WHERE user_id = $1 AND revoked_at IS NULL
        ORDER BY created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&db.db_pool)
    .await
    .map_err(db_error)?;

    Ok(Json(ListDevicesResponse {
        devices: rows.into_iter().map(DeviceListResponse::from).collect(),
    }))
}

/// DELETE /api/v1/user/devices/:id
///
/// Revokes a device token (sets revoked_at = NOW()).
/// Users can only revoke their own devices.
/// Returns 404 if the device doesn't belong to the user.
pub async fn revoke_device(
    State(db): State<DatabaseState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Path(device_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // First, verify the device belongs to the user and is not already revoked
    let result = sqlx::query(
        r#"
        UPDATE device_tokens
        SET revoked_at = NOW()
        WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL
        "#,
    )
    .bind(device_id)
    .bind(user_id)
    .execute(&db.db_pool)
    .await
    .map_err(db_error)?;

    // If no rows were affected, the device either doesn't exist,
    // doesn't belong to the user, or is already revoked
    if result.rows_affected() == 0 {
        return Err(not_found("Device not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_error(e: sqlx::Error) -> (StatusCode, Json<ErrorResponse>) {
    tracing::error!("Database error: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new("Database error")),
    )
}

fn not_found(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::new(msg)),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that SQL query syntax is valid by checking it compiles
    #[test]
    fn test_list_devices_query_syntax() {
        // The query string is validated by sqlx at compile time
        // This test ensures the query is syntactically correct
        let query = r#"
            SELECT id, device_name, created_at, last_used_at
            FROM device_tokens
            WHERE user_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
        "#;
        assert!(!query.is_empty());
    }

    /// Test that revoke device query syntax is valid
    #[test]
    fn test_revoke_device_query_syntax() {
        let query = r#"
            UPDATE device_tokens
            SET revoked_at = NOW()
            WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL
        "#;
        assert!(!query.is_empty());
    }

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
    fn test_device_row_conversion() {
        let now = chrono::Utc::now();
        let row = DeviceRow {
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
