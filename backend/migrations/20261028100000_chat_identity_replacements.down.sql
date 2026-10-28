DROP TABLE IF EXISTS chat_identity_replacement_audit;
DROP INDEX IF EXISTS chat_buzz_membership_operation;
ALTER TABLE chat_buzz_membership_verifications
    DROP CONSTRAINT IF EXISTS chat_buzz_membership_signed_acceptance,
    DROP COLUMN IF EXISTS expected_state,
    DROP COLUMN IF EXISTS operation_id,
    DROP COLUMN IF EXISTS accepted_evaluated_at,
    DROP COLUMN IF EXISTS accepted_event_id,
    DROP COLUMN IF EXISTS accepted_state,
    DROP COLUMN IF EXISTS tenant_id;
DROP TABLE IF EXISTS chat_identity_replacement_commands;
DROP TABLE IF EXISTS chat_identity_replacements;
