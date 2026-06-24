//! Authentication and authorization for RustShare.

pub mod jwt;
pub mod session;

pub use jwt::JwtManager;
pub use rustshare_crypto::{PasswordHasher, DUMMY_HASH};
pub use session::{
    generate_web_session_token, hash_web_session_token, ShareSessionClaims, WEB_SESSION_COOKIE_NAME,
};
