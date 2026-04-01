use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OidcLoginState {
    pub state: String,
    pub pkce_verifier: String,
    pub nonce: String,
    pub redirect_to: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl OidcLoginState {
    /// Create a new OIDC login state.
    ///
    /// All string parameters can be any type that converts into a String,
    /// such as `&str` or `String`.
    pub fn new(
        state: impl Into<String>,
        pkce_verifier: impl Into<String>,
        nonce: impl Into<String>,
        redirect_to: impl Into<String>,
    ) -> Self {
        let now = Utc::now();

        Self {
            state: state.into(),
            pkce_verifier: pkce_verifier.into(),
            nonce: nonce.into(),
            redirect_to: redirect_to.into(),
            expires_at: now + Duration::minutes(10),
            created_at: now,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}
