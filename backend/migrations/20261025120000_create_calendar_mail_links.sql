CREATE TABLE calendar_mail_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_id UUID NOT NULL REFERENCES calendar_events(id) ON DELETE CASCADE,
    mail_message_ref TEXT NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX calendar_mail_links_active_event_uidx
    ON calendar_mail_links(tenant_id, owner_id, event_id)
    WHERE deleted_at IS NULL;

CREATE INDEX calendar_mail_links_active_ref_idx
    ON calendar_mail_links(tenant_id, owner_id, mail_message_ref)
    WHERE deleted_at IS NULL;
