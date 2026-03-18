-- Migration: Create notifications table for in-app notifications

CREATE TABLE IF NOT EXISTS notifications (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  notification_type VARCHAR(50) NOT NULL,
  title VARCHAR(255) NOT NULL,
  message TEXT NOT NULL,
  resource_id UUID NOT NULL,
  resource_type VARCHAR(50) NOT NULL,
  action_url VARCHAR(500),
  read BOOLEAN NOT NULL DEFAULT FALSE,
  created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_user_unread
  ON notifications(user_id, read, created_at);

CREATE INDEX IF NOT EXISTS idx_resource
  ON notifications(resource_id, resource_type);

-- Comments for documentation
COMMENT ON TABLE notifications IS 'Persistent in-app notifications for users';
COMMENT ON COLUMN notifications.resource_id IS 'Polymorphic reference to files/folders/shares (no FK constraint)';
COMMENT ON COLUMN notifications.notification_type IS 'Type: share_received, permission_changed, share_revoked';
