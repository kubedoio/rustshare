//! JSON extractor with built-in validation support.
//!
//! Wraps Axum's `Json` extractor and runs `validator::Validate` on the
//! deserialized payload, returning a `400 Bad Request` with field-level details
//! on validation failure.

use axum::{
    extract::{FromRequest, Request},
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
    Json<T>: FromRequest<S>,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(data) = Json::<T>::from_request(req, state)
            .await
            .map_err(|_err| AppError::bad_request("Invalid JSON payload"))?;

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
