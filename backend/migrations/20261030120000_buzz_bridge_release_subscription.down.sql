DELETE FROM integration_consumer_subscriptions
WHERE consumer_id = 'io.elembra.chat.buzz-bridge.v1'
  AND pattern = 'io.elembra.chat.buzz.admission.released.v1';
