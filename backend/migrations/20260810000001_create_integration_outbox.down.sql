-- Revert ADR-0031 integration outbox (drop in dependency order).

DROP TABLE IF EXISTS integration_reference_effects;
DROP TABLE IF EXISTS integration_consumer_receipts;
DROP TABLE IF EXISTS integration_deliveries;
DROP TABLE IF EXISTS integration_outbox;
