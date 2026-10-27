-- Verification receipts, never a membership authority or binding activation.
CREATE TABLE chat_buzz_membership_verifications (
    request_id UUID PRIMARY KEY,
    community_id UUID NOT NULL,
    pubkey TEXT NOT NULL,
    relay_pubkey TEXT NOT NULL,
    minimum_revision BIGINT NOT NULL CHECK (minimum_revision >= 0),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    accepted_at TIMESTAMPTZ,
    accepted_revision BIGINT,
    CHECK ((accepted_at IS NULL) = (accepted_revision IS NULL))
);
CREATE INDEX chat_buzz_membership_accepted_revision ON chat_buzz_membership_verifications
    (community_id, pubkey, relay_pubkey, accepted_revision DESC)
    WHERE accepted_at IS NOT NULL;
