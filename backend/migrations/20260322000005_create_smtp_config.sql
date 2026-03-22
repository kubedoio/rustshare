CREATE TABLE smtp_config (
    id            UUID PRIMARY KEY CHECK (id = '00000000-0000-0000-0000-000000000002'),
    enabled       BOOL NOT NULL DEFAULT false,
    host          TEXT,
    port          INT,
    username      TEXT,
    password_enc  TEXT,
    from_address  TEXT,
    from_name     TEXT,
    tls_mode      TEXT CHECK (tls_mode IN ('starttls', 'tls', 'none')),
    updated_by    UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
