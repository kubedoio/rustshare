//! JSON extractor with built-in validation support.
//!
//! Wraps Axum's `Json` extractor and runs `validator::Validate` on the
//! deserialized payload, returning a `400 Bad Request` with field-level details
//! on validation failure.

use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    Json,
};
use serde::de::DeserializeOwned;
use validator::Validate;

use super::AppError;

/// A JSON extractor that validates the payload using `validator`.
///
/// Usage in handlers:
/// ```text
/// pub async fn create_folder(
///     ValidatedJson(req): ValidatedJson<CreateFolderRequest>,
/// ) -> impl IntoResponse { /* ... */ }
/// ```
pub struct ValidatedJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let data = match Json::<T>::from_request(req, state).await {
            Ok(Json(data)) => data,
            Err(err) => {
                tracing::warn!(error = %err, "rejected request: invalid JSON payload");
                return Err(AppError::bad_request("Invalid JSON payload"));
            }
        };

        if let Err(validation_errors) = data.validate() {
            let details = format_validation_errors(&validation_errors);
            return Err(AppError::bad_request(format!(
                "Validation failed: {details}"
            )));
        }

        Ok(ValidatedJson(data))
    }
}

/// Flatten validator errors into a human-readable string.
fn format_validation_errors(errors: &validator::ValidationErrors) -> String {
    let mut messages: Vec<String> = Vec::new();
    for (field, field_errors) in errors.field_errors() {
        for err in field_errors {
            let msg = err
                .message
                .as_ref()
                .map(|c| c.to_string())
                .unwrap_or_else(|| format!("invalid {field}"));
            messages.push(format!("{field}: {msg}"));
        }
    }
    messages.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::post,
        Router,
    };
    use serde::Deserialize;
    use tower::ServiceExt;

    #[derive(Deserialize, Validate)]
    struct Payload {
        #[validate(length(min = 1))]
        name: String,
    }

    async fn echo(ValidatedJson(payload): ValidatedJson<Payload>) -> String {
        payload.name
    }

    fn app() -> Router {
        Router::new().route("/", post(echo))
    }

    async fn post_json(body: &str) -> (StatusCode, serde_json::Value) {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let value = serde_json::from_slice(&bytes).expect("body is JSON");
        (status, value)
    }

    #[tokio::test]
    async fn malformed_json_keeps_the_generic_400_body() {
        let (status, body) = post_json("{\"name\": ").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        // The serde reason is logged, never leaked onto the wire.
        assert_eq!(body["error"], "Invalid JSON payload");
        assert_eq!(body.as_object().expect("object body").len(), 1);
    }

    #[tokio::test]
    async fn validation_failure_still_reports_details() {
        let (status, body) = post_json("{\"name\": \"\"}").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"]
            .as_str()
            .expect("error is a string")
            .starts_with("Validation failed:"));
    }
}
