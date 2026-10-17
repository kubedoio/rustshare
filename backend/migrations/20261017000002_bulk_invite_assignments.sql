ALTER TABLE invite_tokens
    ADD COLUMN recipient_display_name TEXT,
    ADD COLUMN bulk_delivery_status TEXT NOT NULL DEFAULT 'sent'
        CHECK (bulk_delivery_status IN ('sending', 'sent', 'failed')),
    ADD COLUMN bulk_delivery_attempted_at TIMESTAMPTZ;

CREATE TABLE invite_token_group_assignments (
    invite_token_id UUID NOT NULL REFERENCES invite_tokens(id) ON DELETE CASCADE,
    group_id UUID NOT NULL,
    PRIMARY KEY (invite_token_id, group_id)
);

CREATE INDEX invite_token_group_assignments_group_id
    ON invite_token_group_assignments(group_id);

