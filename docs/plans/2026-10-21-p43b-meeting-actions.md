# P4.3b — Meeting action items

Baseline: `27cb72f4a974ca0ed28bbe05ac316170406dadb0`.
Parent: #328; epic: #333. This slice does not close either issue.

## Model decision (before implementation)

Use a small meeting-owned PostgreSQL action record. Kanban cards require a board,
column folders, sidecars, object storage, and board authorization. Requiring these
merely to record a meeting action would force unrelated setup and a different
authority onto the user. Reusing their task-shaped fields does not justify these
dependencies. Kanban remains unchanged.

The existing meeting/Files authority owns action access. Every request uses the
existing live meeting authorization; mutations require meeting mutation access.
Assignment records responsibility and never grants meeting or Decision access.
Organization administration grants no private-content access. Related Decisions
remain independently authorized by their owning Application.

## Current capabilities and bounded additions

| Capability | Baseline | P4.3b addition |
| --- | --- | --- |
| Calendar to canonical meeting | Durable unique event association, bidirectional navigation | Reuse unchanged |
| Meeting agenda and Files/Notes links | Existing context, independently authorized references | Reuse unchanged |
| Meeting Decisions | Durable creation, linking, reverse provenance | Optional independently authorized Decision reference |
| Kanban tasks | Board/column-dependent cards | Remains independent |
| Meeting actions | Missing | Create, read, edit, complete, reopen, bounded list |
| Past meeting retrieval | Existing meeting/Decision context | Include persisted actions and current completion state |
| Broad historical search | Deferred | Remains P4.3c |

## Persistence and lifecycle contract

- Stable action ID and immutable originating meeting/event references.
- Title, optional description, authoritative same-tenant enabled responsible user,
  optional due date, explicit open/completed state, version, timestamps and actors.
- Due dates are calendar dates (`YYYY-MM-DD`), not instants: they do not shift when
  the viewer changes timezone. Creation/update/completion timestamps are UTC instants.
- Creation request identity is tenant/actor scoped; identical retries recover the
  same action, changed payloads conflict. Updates compare the expected version;
  stale changed writes conflict, and duplicate submissions are retry-safe.
- Successful retries return the latest independently authorized representation of
  that action, rather than replaying an obsolete snapshot. They emit no new event
  and do not overwrite later edits.
- Action row and content-free lifecycle event commit in one short database
  transaction. There are no object-storage operations inside that transaction.
- Keep action rows and event provenance when meetings or Decisions are deleted.
  Retention does not grant access: inaccessible originating meetings conceal action
  details; inaccessible related Decisions expose no protected metadata.
- Existing meeting folders, metadata sidecars, and canonical event associations
  remain unchanged. No new workspace identity, role, or permission evaluator.

## HTTP and schema contract

`meeting_actions` stores current state and immutable origin IDs;
`meeting_action_commands` uniquely records `(tenant_id, actor_id, request_id)` and
the action/request fingerprint. Neither table cascades with source deletion.
The existing event store records content-free `MeetingActionModified` events with
meeting/action IDs, version, operation, timestamp and actor. Activity retrieval
continues to authorize the meeting folder. Events publish only after commit.

| Endpoint | Contract |
| --- | --- |
| `GET /api/v1/meetings/{id}/actions` | Live meeting View; deterministic `created_at,id` order; one-based pagination capped at 100 |
| `POST /api/v1/meetings/{id}/actions` | Live meeting Edit; required request UUID and title; optional description, assignee, due date and Decision |
| `GET /api/v1/meetings/{id}/actions/{action_id}` | Live meeting View; tenant, meeting and action ID must match |
| `PUT /api/v1/meetings/{id}/actions/{action_id}` | Live meeting Edit; request UUID and expected version; full editable fields and `open`/`completed` state |
| `GET /api/v1/meetings/{id}/action-assignees` | Authorized meeting context; bounded active same-tenant identity choices, ID and display name only |

PUT omits `related_decision_id` to preserve its current provenance, sends null to
clear it, or sends a UUID to set it after independent Decision authorization.
This permits completion when a previously linked Decision is no longer accessible.
Responses expose a Decision ID/title/navigation only after that authorization.
An unchanged disabled/deleted historical assignee remains attributable; assigning
a different responsible person requires a currently enabled same-tenant identity.
Foreign, disabled and nonexistent assignment choices have one generic denial.
Private, foreign and nonexistent meetings are concealed before action lookup.

The additive migration leaves existing rows and sidecars unchanged. A code rollback
leaves action/history tables intact and inert; dropping them is not a rollback
strategy. Backups must retain these tables together with the existing event store
and meeting/Decision sources. Deployment restore/upgrade rehearsal is still required.

## Acceptance and release gates

Disposable PostgreSQL/HTTP tests must cover the complete event → meeting → Decision
→ action → completion → historical reopening journey; assignment validation;
direct/group overlap and revocation; disabled and cross-tenant users; concurrency,
retry, stale versions, rollback, restart, and retained provenance. Frontend tests
cover the Actions workflow and request isolation. Run repository backend/frontend,
SQLx, OpenAPI, integration and protected CI gates. Obtain independent code/security
review and human approval on the exact final head before protected merge.

P4.3c reconstruction/search and P4.4 iCalendar remain outside this PR. Deployment,
backup/restore, upgrade and real-user evidence remain separate institutional gates.
