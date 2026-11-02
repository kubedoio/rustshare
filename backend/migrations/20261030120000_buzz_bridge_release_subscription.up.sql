-- Extend an existing durable Buzz bridge registration without changing its
-- consumer identity or disturbing pending delivery state.
INSERT INTO integration_consumer_subscriptions (consumer_id, pattern)
SELECT consumer_id, 'io.elembra.chat.buzz.admission.released.v1'
FROM integration_consumers
WHERE consumer_id = 'io.elembra.chat.buzz-bridge.v1'
ON CONFLICT (consumer_id, pattern) DO NOTHING;
