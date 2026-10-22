-- Action text stays in its authoritative table; this index only narrows
-- unified-search candidates before live meeting-folder authorization.
CREATE INDEX meeting_actions_search_idx
    ON meeting_actions USING GIN (to_tsvector('simple', title || ' ' || coalesce(description, '')));
