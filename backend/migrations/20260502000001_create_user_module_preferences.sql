-- Per-user module preferences
CREATE TABLE user_module_preferences (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    module_key VARCHAR(50) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, module_key)
);

CREATE INDEX idx_user_module_preferences_user ON user_module_preferences(user_id);
