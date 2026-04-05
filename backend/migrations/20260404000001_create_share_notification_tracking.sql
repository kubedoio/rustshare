-- Track first-access notifications for group shares
CREATE TABLE share_access_notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    share_id UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    notified_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, share_id)
);

CREATE INDEX idx_share_access_notifications_user 
ON share_access_notifications(user_id);
CREATE INDEX idx_share_access_notifications_share 
ON share_access_notifications(share_id);
