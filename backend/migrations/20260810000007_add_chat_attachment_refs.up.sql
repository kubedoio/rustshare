-- Recipient-side Chat attachments (issue #242): the observation index retains
-- the identifier-only `elembra-ref` attachment references carried by each
-- verified Buzz event. The refs are NEVER authority and carry no tenant hint;
-- opening still reauthorizes through the Files owner at read time. Stored as a
-- JSONB array of canonical ResourceRef objects, in event tag order.
ALTER TABLE chat_observed_events
    ADD COLUMN attachment_refs JSONB NOT NULL DEFAULT '[]'::jsonb;
