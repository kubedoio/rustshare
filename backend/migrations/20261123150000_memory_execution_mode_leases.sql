-- Fences embedded and external Memory consumers without changing outbox
-- delivery or receipt identity. Multiple processes may hold leases in one mode.
CREATE TABLE memory_execution_mode_guard (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    active_mode TEXT CHECK (active_mode IS NULL OR active_mode IN ('embedded', 'external')),
    generation BIGINT NOT NULL DEFAULT 0 CHECK (generation >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO memory_execution_mode_guard (singleton) VALUES (TRUE)
ON CONFLICT (singleton) DO NOTHING;

CREATE TABLE memory_execution_mode_leases (
    process_id UUID PRIMARY KEY,
    mode TEXT NOT NULL CHECK (mode IN ('embedded', 'external')),
    generation BIGINT NOT NULL CHECK (generation > 0),
    lease_until TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX memory_execution_mode_leases_active_idx
    ON memory_execution_mode_leases (mode, lease_until);

-- Runtime database roles should receive access to these coordination tables
-- only. Deployment-specific role grants are managed by the deployment layer.
