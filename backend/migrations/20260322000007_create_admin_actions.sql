CREATE TABLE admin_actions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id     UUID REFERENCES users(id) ON DELETE SET NULL,
    action_type  TEXT NOT NULL,
    target_type  TEXT,
    target_id    UUID,
    detail       JSONB,
    performed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX admin_actions_actor_id_idx ON admin_actions(actor_id);
CREATE INDEX admin_actions_performed_at_idx ON admin_actions(performed_at DESC);
CREATE INDEX admin_actions_action_type_idx ON admin_actions(action_type);
