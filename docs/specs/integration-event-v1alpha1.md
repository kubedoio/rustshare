# Specification: Elembra Integration Event v1alpha1

Status: Draft  
Date: 2026-08-07  
Base: CloudEvents 1.0-compatible envelope

## Purpose

Define one transport-neutral event envelope for durable communication between Elembra Applications and Connectors.

This specification does not define the transport. PostgreSQL transactional outbox is the initial transport implementation.

## Envelope

Example:

```json
{
  "specversion": "1.0",
  "id": "01K2...",
  "source": "elembra://io.elembra.files",
  "type": "io.elembra.files.file.updated.v1",
  "subject": "elembra://io.elembra.files/file/01K1...",
  "time": "2026-08-07T19:00:00Z",
  "datacontenttype": "application/json",
  "dataschema": "https://schemas.elembra.io/events/files/file-updated-v1.json",

  "elembraTenant": "01J...",
  "elembraWorkspace": "01J...",
  "elembraActor": "principal:01J...",
  "elembraCorrelation": "01K...",
  "elembraCausation": "01K...",
  "elembraResource": {
    "application": "io.elembra.files",
    "resource_type": "file",
    "resource_id": "01K1...",
    "version": "sha256:..."
  },

  "data": {
    "name": "architecture.md",
    "mime_type": "text/markdown",
    "size": 12420
  }
}
```

## Required fields

CloudEvents-compatible required attributes:

- `specversion`: `1.0`;
- `id`: globally unique event ID;
- `source`: URI identifying the publishing Application/Connector;
- `type`: namespaced event type.

Elembra-required integration attributes:

- `elembraTenant` for tenant-scoped events;
- `elembraWorkspace` when workspace-scoped;
- `elembraCorrelation` for a request/workflow chain;
- `elembraActor` when an attributable Principal initiated the event.

`subject`, `time`, `dataschema`, `elembraCausation` and `elembraResource` are required when semantically applicable.

## Event identity and idempotency

Consumers identify duplicate deliveries by event `id` together with `source`.

At-least-once delivery means a consumer must be safe if the same event arrives more than once.

The event ID is not a business mutation idempotency key. Mutation APIs use their own idempotency key and may publish one or more events.

## Event type ownership

The publishing Application owns the namespace and schema.

Format:

```text
io.elembra.<application-or-connector>.<domain>.<event>.v<major>
```

Examples:

```text
io.elembra.files.file.created.v1
io.elembra.files.file.deleted.v1
io.elembra.mail.message.archived.v1
io.elembra.memory.record.indexed.v1
io.elembra.chat.event.projected.v1
io.elembra.connector.shell.command.captured.v1
```

Do not publish unversioned event types once a schema is consumed cross-Application.

## Schema evolution

While the platform contracts are `v1alpha1`, breaking changes are allowed with coordinated first-party migration.

Once an event type is declared stable:

- additive optional fields may remain within the same event major version;
- breaking semantic/schema changes use a new event type major suffix;
- publishers should not emit two versions indefinitely solely for pre-release compatibility;
- consumers must reject/ignore unknown required semantics safely.

## Data minimization

Events carry enough information to route, project and decide whether a source fetch is needed.

Prefer:

```text
ResourceRef + metadata + provenance
```

over:

```text
full source document/message/event copied into every integration event
```

Sensitive content belongs behind the source Application's authorization/fetch contract unless a specific event contract deliberately requires content.

## Principal and service identity

`elembraActor` identifies the business Principal whose action caused the event.

Transport/workload identity is separate and authenticated by the delivery mechanism/API. A service delivering an event must not be recorded as the human actor unless the service itself initiated the action as a Principal.

Agent-initiated events should identify the Agent Principal and preserve initiating/delegating user/workflow in event data or standardized delegation metadata defined by the Agent contract.

## Correlation and causation

- `elembraCorrelation`: stable ID across one user/workflow operation involving multiple Applications.
- `elembraCausation`: event/request ID that directly caused this event.

This allows traces such as:

```text
user uploads file
  -> file.created
      -> memory record published
          -> indexing completed
```

without pretending the operations are one distributed transaction.

## Resource

`elembraResource` follows ADR-0032.

Consumers treat it as opaque. Its presence does not grant access.

## Outbox persistence

Suggested initial tables/fields:

```text
integration_outbox
  id
  tenant_id
  application_id
  event_type
  event_json
  created_at
  available_at
  claimed_by
  claim_expires_at
  attempt_count
  last_error
  delivered_at
  dead_lettered_at

integration_consumer_receipts
  consumer_id
  source
  event_id
  processed_at
  result_hash/status
  primary key (consumer_id, source, event_id)
```

Exact schema may vary, but required semantics may not.

## Delivery behavior

Dispatcher:

1. claims available rows with a bounded lease;
2. sends/calls the configured consumer transport;
3. records successful delivery or retryable/permanent failure;
4. releases/retries after lease expiry if a worker crashes;
5. exposes queue depth, age, retries and DLQ metrics.

Consumers:

1. authenticate sender/workload;
2. validate event schema/tenant context;
3. check idempotency receipt;
4. apply local transaction/effect;
5. record receipt atomically with the effect where possible;
6. return success only after durable processing.

## Dead letter and repair

A dead-lettered event must be inspectable without exposing secrets. Operators need:

- event ID/type/source;
- tenant/workspace;
- consumer;
- attempts;
- redacted last error;
- first/last attempt timestamps;
- requeue action after correction.

Derived consumers such as Memory also require reconciliation/rebuild paths independent of event replay.

## Observability

Required metrics/log fields:

- outbox pending count/oldest age;
- dispatch latency;
- attempts/retries;
- DLQ count;
- consumer processing latency;
- duplicate deliveries;
- event type/source;
- correlation/causation IDs;
- tenant ID (subject to telemetry privacy policy).

Never log full sensitive `data` by default.

## Non-goals

- exactly-once distributed semantics;
- ordering across all Applications;
- global event-sourcing of every Application from this integration log;
- replacing Application private domain events;
- choosing Kafka/NATS now;
- embedding authorization tokens/content secrets in events.

## Contract tests

- Source mutation and outbox insertion are atomic.
- Offline consumer receives event after returning.
- Duplicate delivery produces one business effect.
- Worker crash/lease expiry recovers delivery.
- Invalid schema/tenant mismatch fails closed.
- Permanent failures enter DLQ with redacted diagnostics.
- ResourceRef event does not allow source fetch without independent authorization.
- Event envelope round-trips without depending on one Rust enum.
