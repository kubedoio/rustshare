//! Buzz event ingestion HTTP handler.
//!
//! `POST /api/v1/integrations/buzz/events` is the observation endpoint of the
//! Buzz → Elembra Memory projection: Buzz pushes a signed chat event, the
//! bridge verifies the HMAC (`X-RustShare-Signature`), the Nostr id/signature,
//! and the community/author mappings, then records the observation and
//! publishes the durable integration event (see
//! [`crate::buzz_observation::BuzzObservationService`]).

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use bytes::Bytes;
use serde_json::{json, Value};
use tracing::{error, warn};

use crate::buzz_observation::{BuzzPushError, IngestOutcome};
use crate::AppState;

const SIGNATURE_HEADER: &str = "X-RustShare-Signature";

/// Error response in the standard `{error, details}` shape used across the
/// handlers (see `handlers/mod.rs`).
fn error_response(status: StatusCode, error: &str, details: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": error, "details": details })))
}

/// POST /api/v1/integrations/buzz/events — authenticated push of a signed Buzz
/// event. HMAC via `X-RustShare-Signature`; replay-window enforced.
pub async fn receive_buzz_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let signature = headers
        .get(SIGNATURE_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    match state
        .buzz_observation_service
        .verify_and_ingest(&body, signature)
        .await
    {
        Ok(IngestOutcome::FirstObservation) => (
            StatusCode::ACCEPTED,
            Json(json!({ "status": "observed", "duplicate": false })),
        ),
        Ok(IngestOutcome::DuplicateObservation) => (
            StatusCode::ACCEPTED,
            Json(json!({ "status": "observed", "duplicate": true })),
        ),
        Err(BuzzPushError::Unauthorized) => {
            warn!("buzz event rejected: invalid HMAC or outside replay window");
            error_response(
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
                &format!("{SIGNATURE_HEADER} is missing, invalid, or outside the replay window"),
            )
        }
        Err(BuzzPushError::Malformed(reason)) => {
            warn!("buzz event rejected: malformed payload ({reason})");
            error_response(StatusCode::BAD_REQUEST, "Malformed request", &reason)
        }
        Err(BuzzPushError::VerificationFailed) => {
            warn!("buzz event rejected: id or signature verification failed, or not a text note");
            error_response(
                StatusCode::FORBIDDEN,
                "Event verification failed",
                "The Nostr event failed id or signature verification, or is not a text note",
            )
        }
        Err(BuzzPushError::UnknownCommunity) => {
            warn!("buzz event rejected: community has no active workspace mapping");
            error_response(
                StatusCode::FORBIDDEN,
                "Unknown community",
                "community_id has no active workspace mapping",
            )
        }
        Err(BuzzPushError::UnboundAuthor) => {
            warn!("buzz event rejected: event author has no active binding in the mapped tenant");
            error_response(
                StatusCode::FORBIDDEN,
                "Unbound author",
                "The event author pubkey has no active binding in the mapped tenant",
            )
        }
        Err(BuzzPushError::AmbiguousCommunity {
            community_id,
            row_count,
        }) => {
            error!(
                "Ambiguous active community mapping for community_id {community_id}: {row_count} tenants"
            );
            error_response(
                StatusCode::CONFLICT,
                "Ambiguous community mapping",
                "community_id has multiple active workspace mappings; reconcile them before retrying",
            )
        }
        Err(BuzzPushError::Persistence(reason)) => {
            error!("Buzz event persistence failure: {reason}");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error",
                "internal error processing buzz event",
            )
        }
    }
}
