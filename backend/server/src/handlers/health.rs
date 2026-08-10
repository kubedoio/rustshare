//! Health and readiness handlers for operational monitoring.

use std::collections::HashMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

use crate::state::AppState;

/// Health of an individual system component.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ComponentHealth {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ComponentHealth {
    fn healthy() -> Self {
        Self {
            status: "healthy".to_string(),
            error: None,
        }
    }

    fn unhealthy(error: impl Into<String>) -> Self {
        Self {
            status: "unhealthy".to_string(),
            error: Some(error.into()),
        }
    }

    fn disabled() -> Self {
        Self {
            status: "disabled".to_string(),
            error: None,
        }
    }
}

/// Operational readiness response.
///
/// Distinct from the lightweight liveness probe (`/health`), this endpoint
/// checks every runtime dependency required for the server to serve traffic.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReadinessResponse {
    pub status: String,
    pub components: HashMap<String, ComponentHealth>,
}

/// Readiness probe endpoint (`GET /ready`).
///
/// Returns `200 OK` when all required dependencies are healthy.
/// Returns `503 Service Unavailable` when any required dependency is unhealthy.
///
/// Components checked:
/// - `database`        – metadata projection DB connectivity
/// - `object_storage`  – S3/RustFS bucket accessibility
/// - `event_delivery`  – event store DB + in-memory broadcaster health
/// - `auth_session`    – JWT signing/verification + session table accessibility
/// - `ai`              – AI service presence (optional; does not fail readiness)
/// - `outbox`          – durable integration-event outbox dispatcher tick
///                       freshness (informational; does not fail readiness)
#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "Admin",
    responses(
        (status = 200, description = "Success", body = ReadinessResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn readiness_check(
    State(state): State<AppState>,
) -> (StatusCode, Json<ReadinessResponse>) {
    let mut components = HashMap::new();

    // ------------------------------------------------------------------
    // Database (metadata projection)
    // ------------------------------------------------------------------
    let db_health = match sqlx::query("SELECT 1")
        .fetch_one(state.metadata_store.pool())
        .await
    {
        Ok(_) => ComponentHealth::healthy(),
        Err(e) => {
            tracing::error!(error = %e, "Readiness probe: database connectivity failed");
            ComponentHealth::unhealthy("database connectivity failed")
        }
    };
    components.insert("database".to_string(), db_health);

    // ------------------------------------------------------------------
    // Object storage
    // ------------------------------------------------------------------
    let storage_health = match state.object_store.health_check().await {
        Ok(_) => ComponentHealth::healthy(),
        Err(e) => {
            tracing::error!(error = %e, "Readiness probe: object storage check failed");
            ComponentHealth::unhealthy("object storage check failed")
        }
    };
    components.insert("object_storage".to_string(), storage_health);

    // ------------------------------------------------------------------
    // Event delivery (event store DB + broadcaster channel)
    // ------------------------------------------------------------------
    let event_db_ok = sqlx::query("SELECT 1")
        .fetch_one(state.event_store.pool())
        .await
        .is_ok();
    let broadcaster_ok = state.broadcaster.is_healthy();

    let event_health = if !event_db_ok {
        ComponentHealth::unhealthy("event store database connectivity failed")
    } else if !broadcaster_ok {
        ComponentHealth::unhealthy("event broadcaster channel is closed")
    } else {
        ComponentHealth::healthy()
    };
    components.insert("event_delivery".to_string(), event_health);

    // ------------------------------------------------------------------
    // Auth / session health
    // ------------------------------------------------------------------
    let auth_health = check_auth_health(&state).await;
    components.insert("auth_session".to_string(), auth_health);

    // ------------------------------------------------------------------
    // AI / search index (optional)
    // ------------------------------------------------------------------
    let ai_health = if state.ai_service.is_some() {
        // TODO: Add deeper AI/index health check once index lag metrics are available.
        ComponentHealth::healthy()
    } else {
        ComponentHealth::disabled()
    };
    components.insert("ai".to_string(), ai_health);

    // ------------------------------------------------------------------
    // Durable integration-event outbox dispatcher (ADR-0031)
    // ------------------------------------------------------------------
    // Optional like `ai`: a stalled or disabled dispatcher does not block
    // request serving (events accumulate durably in the outbox), so this
    // component is informational and never fails overall readiness.
    let outbox_component = check_outbox_health(&state);
    components.insert("outbox".to_string(), outbox_component);

    // ------------------------------------------------------------------
    // Overall readiness
    // ------------------------------------------------------------------
    let (http_status, response) = evaluate_readiness(components);
    (http_status, Json(response))
}

/// Health of the outbox dispatcher: healthy while the last tick completed
/// (`last_tick_ok`) within the configured staleness window; `disabled` when
/// the worker was not spawned; unhealthy otherwise.
fn check_outbox_health(state: &AppState) -> ComponentHealth {
    if !state.outbox_worker_enabled {
        return ComponentHealth::disabled();
    }
    let ok = state
        .outbox_status
        .last_tick_ok
        .load(std::sync::atomic::Ordering::Relaxed);
    let staleness = std::time::Duration::from_secs(state.outbox_readiness_staleness_secs);
    let fresh = state
        .outbox_status
        .last_tick_at
        .lock()
        .ok()
        .is_some_and(|guard| guard.is_some_and(|last_tick| last_tick.elapsed() <= staleness));
    if !ok {
        ComponentHealth::unhealthy("outbox dispatcher has not completed a tick")
    } else if !fresh {
        ComponentHealth::unhealthy("outbox dispatcher last tick is stale")
    } else {
        ComponentHealth::healthy()
    }
}

async fn check_auth_health(state: &AppState) -> ComponentHealth {
    // Verify JWT manager can round-trip a token.
    let jwt_ok = {
        let token =
            state
                .jwt_manager
                .generate(Uuid::nil(), "readiness@rustshare.local", Uuid::nil());
        match token {
            Ok(t) => state.jwt_manager.validate(&t).is_ok(),
            Err(_) => false,
        }
    };

    if !jwt_ok {
        return ComponentHealth::unhealthy("jwt manager round-trip failed");
    }

    // Verify the session table is queryable.
    match sqlx::query("SELECT COUNT(*) FROM user_sessions LIMIT 1")
        .fetch_one(state.metadata_store.pool())
        .await
    {
        Ok(_) => {}
        Err(e) => {
            tracing::error!(error = %e, "Readiness probe: session table query failed");
            return ComponentHealth::unhealthy("session table query failed");
        }
    }

    ComponentHealth::healthy()
}

/// Evaluate overall readiness from individual component healths.
pub fn evaluate_readiness(
    components: HashMap<String, ComponentHealth>,
) -> (StatusCode, ReadinessResponse) {
    let required_ready = [
        "database",
        "object_storage",
        "event_delivery",
        "auth_session",
    ]
    .iter()
    .all(|key| {
        components
            .get(*key)
            .map(|c| c.status == "healthy")
            .unwrap_or(false)
    });

    let status = if required_ready {
        "ready".to_string()
    } else {
        "not_ready".to_string()
    };

    let http_status = if required_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (http_status, ReadinessResponse { status, components })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_health_variants_serialize() {
        let healthy = ComponentHealth::healthy();
        let unhealthy = ComponentHealth::unhealthy("boom");
        let disabled = ComponentHealth::disabled();

        let healthy_json = serde_json::to_value(&healthy).unwrap();
        let unhealthy_json = serde_json::to_value(&unhealthy).unwrap();
        let disabled_json = serde_json::to_value(&disabled).unwrap();

        assert_eq!(healthy_json["status"], "healthy");
        assert!(healthy_json.get("error").is_none());

        assert_eq!(unhealthy_json["status"], "unhealthy");
        assert_eq!(unhealthy_json["error"], "boom");

        assert_eq!(disabled_json["status"], "disabled");
        assert!(disabled_json.get("error").is_none());
    }

    #[test]
    fn test_healthy_readiness_includes_all_required_checks() {
        let mut components = HashMap::new();
        components.insert("database".to_string(), ComponentHealth::healthy());
        components.insert("object_storage".to_string(), ComponentHealth::healthy());
        components.insert("event_delivery".to_string(), ComponentHealth::healthy());
        components.insert("auth_session".to_string(), ComponentHealth::healthy());
        components.insert("ai".to_string(), ComponentHealth::disabled());

        let (http_status, response) = evaluate_readiness(components);
        assert_eq!(http_status, StatusCode::OK);
        assert_eq!(response.status, "ready");
        assert!(response.components.contains_key("database"));
        assert!(response.components.contains_key("object_storage"));
        assert!(response.components.contains_key("event_delivery"));
        assert!(response.components.contains_key("auth_session"));
        assert!(response.components.contains_key("ai"));
    }

    #[test]
    fn test_readiness_response_ready_when_all_required_healthy() {
        let mut components = HashMap::new();
        components.insert("database".to_string(), ComponentHealth::healthy());
        components.insert("object_storage".to_string(), ComponentHealth::healthy());
        components.insert("event_delivery".to_string(), ComponentHealth::healthy());
        components.insert("auth_session".to_string(), ComponentHealth::healthy());
        components.insert("ai".to_string(), ComponentHealth::disabled());

        let (http_status, response) = evaluate_readiness(components);
        assert_eq!(http_status, StatusCode::OK);
        assert_eq!(response.status, "ready");
    }

    #[test]
    fn test_readiness_response_not_ready_when_required_fails() {
        let mut components = HashMap::new();
        components.insert(
            "database".to_string(),
            ComponentHealth::unhealthy("timeout"),
        );
        components.insert("object_storage".to_string(), ComponentHealth::healthy());
        components.insert("event_delivery".to_string(), ComponentHealth::healthy());
        components.insert("auth_session".to_string(), ComponentHealth::healthy());
        components.insert("ai".to_string(), ComponentHealth::disabled());

        let (http_status, response) = evaluate_readiness(components);
        assert_eq!(http_status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.status, "not_ready");
    }

    #[test]
    fn test_disabled_ai_does_not_fail_readiness() {
        let mut components = HashMap::new();
        components.insert("database".to_string(), ComponentHealth::healthy());
        components.insert("object_storage".to_string(), ComponentHealth::healthy());
        components.insert("event_delivery".to_string(), ComponentHealth::healthy());
        components.insert("auth_session".to_string(), ComponentHealth::healthy());
        components.insert("ai".to_string(), ComponentHealth::disabled());

        let (http_status, response) = evaluate_readiness(components);
        assert_eq!(http_status, StatusCode::OK);
        assert_eq!(response.status, "ready");
    }

    #[test]
    fn test_simulated_dependency_failure_returns_not_ready() {
        let mut components = HashMap::new();
        components.insert("database".to_string(), ComponentHealth::healthy());
        components.insert(
            "object_storage".to_string(),
            ComponentHealth::unhealthy("connection refused"),
        );
        components.insert("event_delivery".to_string(), ComponentHealth::healthy());
        components.insert("auth_session".to_string(), ComponentHealth::healthy());
        components.insert("ai".to_string(), ComponentHealth::disabled());

        let (http_status, response) = evaluate_readiness(components);
        assert_eq!(http_status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.status, "not_ready");
    }
}
