-- Login protection: track failed attempts per IP
CREATE TABLE login_attempts (
    ip_address TEXT PRIMARY KEY,
    failed_count INTEGER NOT NULL DEFAULT 0,
    last_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    blocked_until TIMESTAMPTZ
);

CREATE INDEX idx_login_attempts_blocked ON login_attempts(blocked_until) WHERE blocked_until IS NOT NULL;

-- Security configuration (single-row table enforced by CHECK constraint)
CREATE TABLE security_config (
    id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    login_protection_enabled BOOLEAN NOT NULL DEFAULT true,
    max_login_attempts INTEGER NOT NULL DEFAULT 5,
    login_block_duration_minutes INTEGER NOT NULL DEFAULT 15,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed the default security config row
INSERT INTO security_config (id, login_protection_enabled, max_login_attempts, login_block_duration_minutes)
VALUES (1, true, 5, 15);
