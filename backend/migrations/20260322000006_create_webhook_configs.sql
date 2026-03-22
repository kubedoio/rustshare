CREATE TABLE webhook_configs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    url         TEXT NOT NULL,
    secret_enc  TEXT,
    enabled     BOOL NOT NULL DEFAULT true,
    events      TEXT[] NOT NULL CHECK (events <@ ARRAY['file.uploaded','file.deleted','file.restored','folder.created','folder.deleted','share.created','share.revoked','user.created','user.disabled','user.deleted']) CHECK (array_length(events, 1) > 0),
    created_by  UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
