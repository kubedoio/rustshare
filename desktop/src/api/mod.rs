//! API client for RustShare server
//!
//! Provides HTTP client for server API including:
//! - Device authentication
//! - File upload/download
//! - Delta sync
//! - WebSocket real-time sync

pub mod auth;
pub mod client;
pub mod upload;

pub use client::{ApiClient, ApiError};
pub use auth::{DeviceAuth, DeviceToken};
