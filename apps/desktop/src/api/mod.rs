//! API client for RustShare server
//!
//! Provides HTTP client for server API including:
//! - Device authentication
//! - File upload/download
//! - Delta sync
//! - WebSocket real-time sync

pub mod auth;

pub use auth::{DeviceAuth, DeviceToken};
