-- Meeting-owned action history survives source deletion. These references deliberately
-- have no cascading foreign keys; access always reauthorizes the live meeting.
CREATE TABLE meeting_actions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    meeting_id UUID NOT NULL,
    calendar_event_id UUID,
    title TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 255),
    description TEXT,
    assignee_id UUID,
    due_date DATE,
    state TEXT NOT NULL CHECK (state IN ('open', 'completed')),
    related_decision_id UUID,
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    created_by UUID NOT NULL,
    updated_by UUID NOT NULL,
    completed_at TIMESTAMPTZ,
    completed_by UUID,
    CHECK ((state = 'open' AND completed_at IS NULL AND completed_by IS NULL)
        OR (state = 'completed' AND completed_at IS NOT NULL AND completed_by IS NOT NULL))
);
CREATE INDEX meeting_actions_meeting_idx ON meeting_actions (tenant_id, meeting_id, created_at, id);

-- A command and its action/event commit together. Retries never duplicate mutations.
CREATE TABLE meeting_action_commands (
    tenant_id UUID NOT NULL,
    actor_id UUID NOT NULL,
    request_id UUID NOT NULL,
    action_id UUID NOT NULL,
    payload_hash TEXT NOT NULL,
    PRIMARY KEY (tenant_id, actor_id, request_id)
);
