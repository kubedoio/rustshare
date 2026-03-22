CREATE TABLE admin_actions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id     UUID REFERENCES users(id) ON DELETE SET NULL,
    action_type  TEXT NOT NULL,
    target_type  TEXT,
    target_id    UUID,
    detail       JSONB,
    performed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_admin_actions_actor_id ON admin_actions(actor_id);
CREATE INDEX idx_admin_actions_performed_at ON admin_actions(performed_at DESC);
CREATE INDEX idx_admin_actions_action_type ON admin_actions(action_type);
CREATE INDEX idx_admin_actions_target_id ON admin_actions(target_id) WHERE target_id IS NOT NULL;
