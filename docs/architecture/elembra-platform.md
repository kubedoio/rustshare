# Elembra Platform Architecture

Status: Target architecture  
Date: 2026-08-07  
Applies to: migration from RustShare to Elembra

## 1. Product definition

Elembra is a **Sovereign Business Memory Platform**: one product experience composed of independently owned Applications that preserve durable business context, provenance and access control.

Initial first-party Applications:

- **Elembra Files** — files, folders, versions and sharing.
- **Elembra Notes** — durable file-backed knowledge and editing workflows.
- **Elembra Mail** — IMAP/SMTP communication and durable mail records.
- **Elembra Chat** — live signed communication, powered by Buzz.
- **Elembra Memory** — catalog, indexing, search and permission-aware retrieval/RAG.
- **Elembra Agents** — agent identities, runs, delegated actions and approvals.

Optional/later Applications can include Decisions, Meetings, Kanban and Object Spaces when they justify an independent product surface.

Elembra is not a generic plugin host. The architecture is optimized first for a coherent first-party product and only later for third-party extensibility.

## 2. Vocabulary

### Application

A user-visible or domain-significant Elembra product boundary with explicit ownership of data, APIs, actions, events and UI contributions.

An Application's runtime strategy is separate from its identity:

- `embedded`: compiled into the Elembra backend today;
- `service`: independently deployed service/worker;
- `bridge`: Elembra adapter around an independently coherent external Engine.

Changing runtime strategy must not change the Application's resource identities or product meaning.

### Connector

An integration with an external source or sink. Examples: Google Drive, OneDrive, Dropbox, local editor folders, Obsidian-compatible vaults, shell history and GitHub.

A Connector is not a transparent implementation of Elembra Files. It preserves the semantics and authority of the external source and declares which modes it supports: reference, mirror, import, archive, export, or bidirectional sync.

### Engine

An independently coherent runtime used behind an Elembra Application. Buzz is the Engine behind Elembra Chat.

### Contribution

Declarative metadata used by the Elembra Shell to compose navigation, routes, commands, dashboard cards, settings and search surfaces.

### Extension

Future third-party executable code loaded under an explicit sandbox and public extension contract. No public Extension runtime is part of the initial architecture.

### Contract

A typed synchronous API schema or durable integration-event schema between boundaries.

### Action capability

A fine-grained permission/delegation verb such as `files.read`, `mail.send`, `chat.post` or `memory.query`. The word capability is reserved for actions/authority, not for top-level product Applications.

## 3. Architectural overview

```text
┌───────────────────────────────────────────────────────────────────────┐
│                         Elembra Shell                                 │
│                                                                       │
│ Files │ Notes │ Mail │ Chat │ Memory/Search │ Agents │ Settings       │
│ Navigation · Commands · Dashboard · Search composition · Admin        │
└───────────────────────────────┬───────────────────────────────────────┘
                                │ authenticated Application APIs
                                │
┌───────────────────────────────▼───────────────────────────────────────┐
│                      Elembra Platform Core                            │
│                                                                       │
│ Tenant / Workspace identity                                           │
│ Principals: users · groups · service accounts · agents                 │
│ OIDC identity mapping                                                  │
│ Application registry / enablement / configuration                      │
│ Global roles and Application grants                                    │
│ Service/workload identity                                              │
│ Audit foundation                                                       │
│ Resource-reference conventions                                         │
└───────────────┬──────────────────┬──────────────────┬─────────────────┘
                │                  │                  │
      typed APIs│        durable events               │principal context
                │                  │                  │
    ┌───────────▼─────┐  ┌────────▼────────┐  ┌──────▼────────────┐
    │ Elembra Files   │  │ Elembra Memory │  │ Elembra Mail     │
    │ + Notes         │  │ Catalog         │  │                  │
    │                 │  │ Index/Search    │  │                  │
    └───────────┬─────┘  │ Retrieval/RAG   │  └──────────────────┘
                │        └────────┬────────┘
                │                 │
                │                 │
                │        ┌────────▼────────┐
                │        │ Elembra Agents │
                │        └─────────────────┘
                │
                │ ResourceRef / events
                │
       ┌────────▼──────────────────────────────────────┐
       │              Elembra Chat                    │
       │  Application contract + Elembra Chat Bridge │
       └───────────────────────┬───────────────────────┘
                               │ Buzz protocol / API / SDK
                         ┌─────▼─────┐
                         │   Buzz    │
                         │  Engine   │
                         │ Relay/UI  │
                         └───────────┘

External systems
  Google Drive · OneDrive · Dropbox · local editors · shell · GitHub
                 │
                 └──────── Elembra Connectors ────────► Applications/Memory
```

## 4. Platform Core boundary

The Platform Core must remain deliberately small. It must not become the old `AppState` recreated across network boundaries.

### Core owns

- canonical `TenantId` and `WorkspaceId` identities and lifecycle;
- canonical `PrincipalId` for users, groups, service accounts and agents;
- OIDC issuer/subject to Principal mapping;
- workspace membership and global roles;
- Application registration, enablement and configuration references;
- Application-level grants;
- workload/service identity used for Application-to-Application calls;
- audit-event ingestion/foundation;
- canonical `ResourceRef` syntax and ownership resolution;
- shared correlation/causation identifiers.

### Core does not own

- file/folder business logic;
- Markdown editing;
- IMAP/SMTP state;
- Buzz/Nostr event storage;
- chat channels/messages/workflows;
- embeddings or vector indexes;
- RAG orchestration;
- agent model/provider implementation;
- Object Spaces IAM;
- Application-specific tables.

## 5. Application ownership

Every Application owns:

- its domain model;
- authoritative resource-level authorization;
- private database tables/schema;
- migrations;
- resource identifiers;
- synchronous API surface;
- published event schemas;
- consumed event handlers;
- audit details;
- export/deletion/retention semantics;
- health/readiness when independently deployed.

No Application may query another Application's private tables.

A shared PostgreSQL cluster is acceptable. Logical ownership is still mandatory. Service-backed Applications should have separate schemas and database roles where practical.

## 6. Application data ownership

| Application | Source of truth |
|---|---|
| Platform Core | tenants, workspaces, principals, memberships, Application registry/grants |
| Files | file/folder namespace, versions, shares, artifact metadata and object references |
| Notes | note-specific metadata, templates, backlinks/projections; note content may be a Files artifact |
| Mail | accounts, folders, UID/UIDVALIDITY state, flags, sync cursors, jobs, drafts, send state |
| Chat | Buzz-owned signed events, channels, memberships, chat workflows and chat-specific state |
| Memory | memory catalog records, indexing state, chunks/embeddings/search projections; never original source authority |
| Agents | agent definitions, grants, runs, approvals, traces, provider configuration and usage |
| Object Spaces (optional) | S3 spaces, scoped S3 identities, quotas and publishing policy |

## 7. Application manifest

Every first-party Application has a declarative manifest. The manifest describes the Application but does not load executable code.

The manifest declares:

- identity and version;
- runtime strategy;
- provided and required contracts;
- user/action capabilities;
- owned resource types;
- published/subscribed integration events;
- UI contributions;
- memory publication/search contributions;
- configuration schema reference;
- data ownership/export/preservation policy;
- health endpoint for service/bridge runtimes.

Canonical specification: `docs/specs/application-manifest-v1alpha1.md`.

## 8. Synchronous Application contracts

Use a small number of explicit typed contracts.

Initial transport policy:

- embedded Application: Rust trait/adapter implementing contract semantics;
- service/bridge Application: HTTP/JSON API described by OpenAPI/JSON Schema;
- generated clients/types where valuable;
- authenticated workload identity + delegated Principal context;
- correlation ID on every call;
- idempotency key on retryable mutations.

Do not require gRPC, protobuf or protocol negotiation merely to imitate mature plugin systems. Introduce another transport only for a demonstrated performance/streaming need.

All initial contracts are `v1alpha1`. Compatibility is intentionally weak until first-party use proves them.

## 9. Durable integration events

The existing in-memory `EventBroadcaster` remains useful for ephemeral realtime UI notifications. It is not a reliable cross-Application bus.

Cross-Application events use:

1. the same database transaction as the authoritative domain mutation writes an outbox record;
2. an outbox dispatcher claims pending records;
3. events are delivered at least once;
4. each consumer stores idempotency/checkpoint state;
5. permanent failures go to a dead-letter state with operator visibility;
6. consumers reconcile from source APIs if events were delayed or duplicated.

Initial implementation uses PostgreSQL. Kafka/NATS is not a prerequisite and can replace the transport later without changing event contracts.

Event envelopes are CloudEvents 1.0-compatible and use namespaced string types, e.g.:

```text
io.elembra.files.file.updated.v1
io.elembra.mail.message.archived.v1
io.elembra.chat.event.projected.v1
```

A central Rust `EventType` enum must not be the public integration schema registry.

Canonical specification: `docs/specs/integration-event-v1alpha1.md`.

## 10. Resource references

Cross-Application relationships store opaque references, not foreign-table keys.

Canonical form:

```text
ResourceRef {
  application: "io.elembra.files",
  resource_type: "file",
  resource_id: "01J...",
  version: "sha256:..." | null
}
```

URI rendering may be used for transport/display:

```text
elembra://io.elembra.files/file/01J...?version=sha256%3A...
```

Rules:

- the owning Application is the only authority for resource data and resource-level authorization;
- indexes/cache/search results are never authorization authorities;
- opening/materializing a ResourceRef reauthorizes against the owner;
- immutable version references are preferred for provenance;
- deleted/revoked resources remain referentially explainable without leaking content.

## 11. Authorization model

### Platform authorization

Core establishes:

```text
PrincipalContext
- principal_id
- tenant_id
- workspace_id
- groups
- global/application grants
- authentication strength/context
- delegation chain
- correlation_id
```

### Resource authorization

The owning Application decides whether the Principal can perform an action on a resource.

Examples:

```text
Files.authorize(principal, file_ref, files.read)
Mail.authorize(principal, message_ref, mail.read)
Chat.authorize(principal, channel/event_ref, chat.read)
```

Cross-Application calls never infer access from raw IDs or shared database visibility.

### Batch authorization

Search/RAG needs efficient checks. Applications must expose a bounded batch-authorize contract so Memory can filter candidates without N+1 calls.

The source Application must reauthorize selected sources immediately before content is materialized for an LLM.

## 12. Elembra Memory

Memory is an Application/platform service, not part of Platform Core.

It has three conceptual layers.

### Memory Catalog

Stores references and provenance:

- tenant/workspace;
- source Application/type/ref/version;
- actor/author mapping;
- occurred/observed timestamps;
- classification;
- retention/legal-hold reference;
- checksum/content fingerprint;
- authorization owner/reference;
- provenance chain;
- indexing status.

The catalog is not a copy of every source database.

### Index/Search

Rebuildable projections:

- parsed text;
- chunks;
- PostgreSQL full-text initially;
- pgvector initially;
- structured metadata;
- coarse ACL projection for candidate filtering.

### Retrieval/RAG

1. accept PrincipalContext and query scope;
2. retrieve candidates using tenant/workspace/coarse ACL filters;
3. batch-reauthorize candidates with source Applications;
4. fetch/materialize only authorized source content;
5. rerank/build context;
6. generate answer;
7. cite ResourceRefs and provenance;
8. audit which source resources were accessed.

No unauthorized content may be sent to a model and filtered afterward.

## 13. Connectors

Connectors integrate external systems without pretending those systems share Elembra Files semantics.

A Connector declares a capability matrix such as:

```text
read/list/watch/write/delete/version/share
reference/mirror/import/archive/export/bidirectional-sync
```

Examples:

- Google Drive Connector;
- OneDrive Connector;
- Dropbox Connector;
- Local Text Connector for Sublime Text / Notepad++ folders;
- Vault/Obsidian-compatible Connector;
- Shell Memory Connector;
- GitHub Connector.

Connector rules:

- own external credentials/cursors/device state;
- use Core principal/workload identity;
- never query Application private tables;
- ingest through public Application/Memory contracts;
- preserve external IDs/provenance;
- explicitly document conflict/deletion semantics;
- support safe retries and idempotency;
- expose health/status.

## 14. Elembra Chat and Buzz

Elembra Chat is an Elembra Application. Buzz is its Engine.

```text
Elembra Shell
    │
    ▼
Elembra Chat Application
    │
    ├── Elembra identity/resource/search/memory contracts
    │
    ▼
Elembra Chat Bridge
    │
    ▼
Buzz public protocol/API/SDK
    │
    ▼
Buzz Relay / clients
```

Hard rules:

- Buzz does not import Elembra Core or query Elembra databases.
- Elembra does not become the source of truth for Buzz chat events.
- no shared Buzz/Elembra database schema;
- Buzz's cryptographic event signing remains authoritative for Buzz events;
- Elembra OIDC identity does not replace NIP-42/NIP-98 authentication;
- audited tenant-scoped `PrincipalId ↔ Buzz pubkey` binding connects the identity systems;
- Elembra workspace ↔ Buzz community mapping is explicit;
- disabling an Elembra user revokes Chat admission/membership even if the user still possesses a valid Buzz key;
- file attachments use ResourceRefs and are reauthorized at fetch time;
- Memory stores references/projections with provenance rather than becoming the chat event store;
- integrations are retryable/eventually consistent, never distributed transactions.

## 15. Agents

Agents are Principals, not privileged backend shortcuts.

An agent receives explicit delegated action capabilities:

```text
User/workflow
  -> delegation
      -> Agent Principal
          files.read
          memory.query
          notes.write
          mail.draft
          chat.post
```

Every agent action records:

- agent Principal;
- initiating user/workflow;
- delegation/grant;
- target Application/action;
- source evidence/ResourceRefs;
- approval where required;
- result/error;
- correlation/causation chain.

`read`, `write`, `delete`, `share`, `send`, and permission-management grants remain distinct.

## 16. Elembra Files and external content

Elembra Files is the native artifact Application.

Do not build a single transparent storage interface where Google Drive, Dropbox, OneDrive and native S3 are interchangeable implementations. Their identity, sharing, ACL, versioning, conflict, rate-limit and deletion semantics are materially different.

Native Files storage remains authoritative for Elembra-native resources. External systems are exposed through Connectors and can be referenced, mirrored or imported explicitly.

## 17. Object Spaces

Object Spaces may become an optional Application for technical teams needing controlled S3 publishing.

It must remain separate from Elembra Files internal object namespaces and metadata contracts. External S3 clients can never mutate native Files blobs/metadata out of band.

Object Spaces is not required for the core Sovereign Business Memory product and is not a dependency of Files, Memory or Chat.

## 18. Shell composition

The Elembra Shell owns shared chrome and composes declarative Application Contributions.

Applications may contribute:

- primary/secondary navigation;
- routes;
- create actions and commands;
- dashboard cards;
- settings/admin pages;
- search providers;
- content renderers;
- health/status surfaces.

Contributions declare known renderer/component keys. Initial first-party Applications do not inject arbitrary remote JavaScript into the shell.

## 19. Failure model

Applications must degrade independently.

Examples:

- Chat outage does not prevent Files/Notes/Mail use.
- Memory/indexing outage does not prevent source Applications from serving authoritative content.
- Connector outage does not corrupt imported/mirrored artifacts.
- Files outage may prevent opening Files-backed Chat attachments, but must not corrupt Buzz history.

No distributed transaction spans Applications.

Patterns:

- local transaction + outbox;
- idempotent consumers;
- retry with bounded backoff;
- dead-letter/operator visibility;
- reconciliation jobs;
- explicit degraded health.

## 20. Migration policy

Elembra does not carry a long-term RustShare internal/API compatibility obligation.

The migration is a deliberate cutover:

- `Module` product terminology becomes `Application`;
- database module records are migrated to Application records;
- public module routes/API contracts are replaced rather than indefinitely aliased;
- old serde aliases and legacy-module contract tests are removed once the migration is complete;
- user content, IDs and durable data are preserved where meaningful;
- existing accepted ADRs that conflict with this architecture are explicitly superseded;
- first-party contracts remain `v1alpha1` until exercised by real Applications.

Compatibility for exported user data matters. Compatibility for obsolete pre-release internal abstractions does not.

## 21. Implementation sequence

### Phase 0 — Architecture and release boundary

- finalize this architecture and governing ADR/specs;
- keep the RustShare v0.7 stabilization tracker separate from Elembra implementation;
- establish one migration branch/series after architecture review.

### Phase 1 — Application foundation inside the current deployment

- create contract/application-registry crates/modules;
- replace `Module` with `Application` through one migration;
- convert existing module metadata into Application manifests/contributions;
- define PrincipalContext, ResourceRef and action capabilities;
- keep first-party Applications embedded;
- reduce handlers' dependency on broad `ServiceState`/`AppState`.

### Phase 2 — Durable integration foundation

- implement Postgres transactional outbox;
- implement CloudEvents-compatible integration envelope;
- implement idempotent consumer/checkpoint/dead-letter infrastructure;
- keep `EventBroadcaster` only for ephemeral live UI fan-out.

### Phase 3 — Prove an external boundary

Do **not** extract Mail first.

Prove service/connector contracts with one naturally asynchronous component:

- Memory indexing worker, or
- Shell/Local Text Connector worker.

Validate service identity, retries, authorization, health, contracts and independent failure.

### Phase 4 — Memory Application

- catalog;
- indexing/search;
- source authorization/batch authorization;
- cited RAG;
- Files/Notes/Mail producers.

### Phase 5 — Elembra Chat around Buzz

- separate Elembra Chat repository/deployment strategy;
- principal↔pubkey and workspace↔community contracts;
- SSO/pairing design without replacing Buzz signatures;
- ResourceRef attachments/unfurls;
- memory/search projection;
- agent delegation;
- upstream compatibility tests.

### Phase 6 — Agents

- explicit agent principals and delegated actions;
- approvals/traces/cost/evaluation;
- no private-table access.

### Phase 7 — Public Extensions only if justified

- evaluate WASM/WASI/Extism;
- define signed packages/trust/update policy;
- stabilize public SDK/ABI;
- consider marketplace.

## 22. Architectural invariants

A change is architecturally invalid if it violates any of these:

1. An Application reads another Application's private database tables.
2. A search/vector index is treated as authorization authority.
3. A cross-Application event can be permanently lost because a consumer was offline.
4. Buzz depends on Elembra private internals.
5. Elembra stores a second authoritative copy of Buzz chat state merely for integration convenience.
6. An OIDC session is treated as permission to impersonate a Buzz private key.
7. External storage providers are forced behind a fake identical Files semantic contract.
8. Agent identity is collapsed into the initiating user's identity.
9. Disabling an Application silently deletes user data.
10. A public third-party ABI is frozen before first-party contracts are proven.
11. Platform Core absorbs Application-specific business logic.
12. Runtime deployment strategy becomes part of resource identity.

## 23. Target design statement

> **Elembra is one sovereign product composed of Applications. Platform Core establishes identity, tenancy and shared authority context. Applications own their domains and data. Memory preserves discoverable provenance without stealing source ownership. Connectors integrate external systems honestly. Buzz remains the signed communication Engine behind Elembra Chat. Stable contracts, ResourceRefs and durable events connect the system without shared-table shortcuts.**
