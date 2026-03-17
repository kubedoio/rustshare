//! Authentication and authorization for RustShare.

pub mod password;
pub mod jwt;
// Note: session module will be added in Phase 2 for session management

pub use password::PasswordHasher;
pub use jwt::JwtManager;
