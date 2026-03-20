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
    pub fn new(state: String, pkce_verifier: String, nonce: String, redirect_to: String) -> Self {
        let now = Utc::now();

        Self {
            state,
            pkce_verifier,
            nonce,
            redirect_to,
            expires_at: now + Duration::minutes(10),
            created_at: now,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}
