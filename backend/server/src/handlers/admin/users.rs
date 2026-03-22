//! Admin user management handlers.
use axum::{extract::State, http::StatusCode, Json};
use crate::{handlers::AdminUser, AppState};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// TODO: implement handlers in Task 4
