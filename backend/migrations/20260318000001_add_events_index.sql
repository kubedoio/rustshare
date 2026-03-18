-- Index for efficient catch-up queries
-- Optimizes: WHERE user_id = $1 AND (timestamp, id) > (...)
CREATE INDEX idx_events_user_timestamp_id ON events(user_id, timestamp, id);
