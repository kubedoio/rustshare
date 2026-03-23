CREATE TABLE device_pair_requests (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_code TEXT NOT NULL UNIQUE,
    user_code   TEXT NOT NULL UNIQUE,
    user_id     UUID REFERENCES users(id) ON DELETE CASCADE,
    expires_at  TIMESTAMPTZ NOT NULL,
    approved_at TIMESTAMPTZ
);

CREATE INDEX idx_device_pair_requests_device_code ON device_pair_requests(device_code);
CREATE INDEX idx_device_pair_requests_user_code ON device_pair_requests(user_code);
CREATE INDEX idx_device_pair_requests_expires_at ON device_pair_requests(expires_at);
