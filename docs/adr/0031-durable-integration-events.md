# ADR-0031: Durable Cross-Application Integration Events

Status: Proposed  
Date: 2026-08-07

## Context

RustShare currently has persisted domain events and an in-memory Tokio `EventBroadcaster`. The broadcaster is appropriate for low-latency WebSocket/UI fan-out, but it is not a reliable integration mechanism: consumers can be absent, lag, restart, or run in another process.

Elembra will have independently owned Applications, Connectors and bridge services. Once a mutation in Files must update Memory or notify Chat, losing an event because a consumer was offline is unacceptable.

Requiring Kafka, NATS or another external broker before the Application model is proven would add operational complexity without solving a contract problem.

## Decision

Elembra adopts a **transactional PostgreSQL outbox** as the first durable integration transport and a **CloudEvents 1.0-compatible envelope** as the logical event contract.

### Separation of concerns

- **Domain/internal events** may remain strongly typed inside an Application.
- **Realtime UI events** may continue using `EventBroadcaster`; they are ephemeral notifications and can be reconstructed/refetched.
- **Integration events** are durable, namespaced, versioned contracts intended for other Applications/Connectors.

These are not interchangeable merely because all are called events.

## Transactional outbox

For an authoritative state change that publishes an integration event:

```text
BEGIN
  write Application state
  write outbox row
COMMIT
```

The mutation must never commit successfully while the corresponding required integration-event record is omitted.

An asynchronous dispatcher then claims and delivers outbox rows.

Initial properties:

- at-least-once delivery;
- consumer idempotency;
- bounded retry/backoff;
- lease/claim ownership so multiple dispatchers can run safely;
- dead-letter state after configured permanent/exhausted failures;
- operator-visible last error/attempt count;
- correlation and causation propagation;
- retention/compaction policy after successful delivery.

Exactly-once delivery is not promised. Business effects that need deduplication use event IDs/idempotency keys.

## Event type names

Integration-event types are namespaced strings:

```text
io.elembra.files.file.created.v1
io.elembra.files.file.updated.v1
io.elembra.files.share.revoked.v1
io.elembra.mail.message.archived.v1
io.elembra.chat.event.projected.v1
```

The public integration registry must not be one central Rust `enum EventType`. A central enum forces unrelated Application releases to modify one core package and turns Core into the owner of every domain event.

Applications may still use enums internally and map selected events to integration schemas.

## Envelope

Canonical schema: `docs/specs/integration-event-v1alpha1.md`.

CloudEvents fields are retained (`id`, `source`, `specversion`, `type`, `subject`, `time`, `datacontenttype`, `dataschema`, `data`) and Elembra extension attributes carry tenant/workspace/principal/resource/correlation metadata.

The envelope contains routing/provenance context. It is not a substitute for Application-owned resource data.

## Consumers

Each durable consumer must define:

- stable consumer identity;
- subscribed event types;
- idempotent effect semantics;
- checkpoint/delivery record;
- retry classification;
- dead-letter behavior;
- reconciliation strategy.

A consumer must tolerate duplicate and delayed events.

## Reconciliation

Events improve timeliness but do not eliminate reconciliation.

For derived/rebuildable systems such as Memory indexing, the source Application must expose enough API/state to repair missed or corrupt projections. The system should be able to rebuild indexes from authoritative resources and the Memory Catalog.

## Transport evolution

PostgreSQL is the initial storage/delivery mechanism because it is already required and supports atomic outbox writes.

A later deployment may route the same integration contracts through NATS, Kafka, Redpanda, cloud queues or another broker if scale/operational needs justify it. Event schemas and Application ownership must not depend on that choice.

## Security

- Tenant/workspace metadata is validated by publishers and consumers.
- Workload identity authenticates service delivery/fetch calls.
- Events contain only the minimum safe data needed for routing/projection.
- Sensitive source content should normally be represented by `ResourceRef`, not copied wholesale into every event.
- Consumers must not treat possession of an event as authorization to fetch its referenced resource.

## Consequences

### Positive

- Service extraction no longer risks silent integration-event loss.
- Applications can fail/restart independently.
- No new broker is required for the initial architecture.
- Event contracts are decoupled from one central enum and implementation language.

### Negative

- At-least-once semantics require idempotent consumers.
- Outbox cleanup/leases/DLQ add operational state.
- Event contracts must be reviewed as APIs rather than casual internal structs.

## Rejected alternatives

### Use only `EventBroadcaster`

Rejected because it is process-local and intentionally allows lag/loss for consumers.

### Use distributed transactions

Rejected. Applications must not require atomic transactions across independent sources of truth.

### Require Kafka/NATS immediately

Rejected. It adds infrastructure before throughput/retention requirements justify it and does not replace the need for transactional publication from the source database.

## Acceptance criteria

- [ ] Integration-event schema exists and is transport-neutral.
- [ ] Outbox write is atomic with representative source mutations.
- [ ] Dispatcher supports lease/retry/DLQ behavior.
- [ ] Consumer idempotency is contract-tested.
- [ ] `EventBroadcaster` is documented as ephemeral UI fan-out only.
- [ ] Integration event types are namespaced strings owned by Applications.
- [ ] A consumer can be offline during a source mutation and process the event after recovery.
