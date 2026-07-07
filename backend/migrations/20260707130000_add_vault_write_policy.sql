-- Add explicit write policy to vaults. Default read_only keeps existing vaults safe.
ALTER TABLE vaults ADD COLUMN IF NOT EXISTS write_policy VARCHAR(50) NOT NULL DEFAULT 'read_only';

-- Constraint to enforce known values without a separate enum type.
ALTER TABLE vaults DROP CONSTRAINT IF EXISTS vaults_write_policy_check;
ALTER TABLE vaults ADD CONSTRAINT vaults_write_policy_check
    CHECK (write_policy IN ('read_only', 'web_editing_enabled', 'sync_client_only'));
