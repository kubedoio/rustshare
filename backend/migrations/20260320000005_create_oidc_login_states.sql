CREATE TABLE oidc_login_states (
    state VARCHAR(255) PRIMARY KEY,
    pkce_verifier TEXT NOT NULL,
    nonce VARCHAR(255) NOT NULL,
    redirect_to TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_oidc_login_states_expires_at
    ON oidc_login_states (expires_at);
