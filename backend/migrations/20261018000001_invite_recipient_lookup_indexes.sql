CREATE INDEX invite_tokens_pending_recipient_lower_idx
    ON invite_tokens (lower(recipient_email), created_at DESC)
    WHERE used_at IS NULL AND revoked_at IS NULL;

CREATE INDEX invite_tokens_expired_recipient_lower_idx
    ON invite_tokens (tenant_id, lower(recipient_email), expires_at)
    WHERE used_at IS NULL AND revoked_at IS NULL;
