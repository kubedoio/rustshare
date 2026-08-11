-- Optional pinned public key of the community's authoritative Buzz relay,
-- trusted when asking it for source-authorization decisions. The CHECK mirrors
-- the v1alpha1 upstream spec: exactly 64 lowercase hex (a Nostr x-only key).
ALTER TABLE chat_workspace_communities
    ADD COLUMN relay_pubkey TEXT
    CHECK (relay_pubkey IS NULL OR relay_pubkey ~ '^[0-9a-f]{64}$');
