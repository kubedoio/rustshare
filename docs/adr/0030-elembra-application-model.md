# ADR-0030: Elembra Application Model

Status: Proposed  
Date: 2026-08-07  
Supersedes in part: ADR-0016 for product/module boundaries

## Context

RustShare currently uses `Module` as both a UI/content-template concept and an informal product boundary. At runtime, domain services are wired directly into shared `ServiceState` / `AppState`. This is acceptable for an MVP but does not provide ownership boundaries for Files, Mail, Memory, Chat and Agents.

Calling all future extensibility a `plugin` would also collapse several materially different concepts into one word: first-party products, external data integrations, independent engines, UI contributions and future untrusted executable extensions.

Elembra has no existing public third-party plugin ecosystem and no requirement to keep the pre-release Module API stable.

## Decision

Elembra adopts **Application** as the top-level logical and product boundary.

### Application identity is independent of deployment

An Application has one stable identity, for example:

```text
io.elembra.files
io.elembra.notes
io.elembra.mail
io.elembra.memory
io.elembra.chat
io.elembra.agents
```

Its runtime strategy is metadata:

```text
embedded | service | bridge
```

Moving an Application from embedded code to an independent service must not change its resource identities, permissions, event names or product meaning.

### Other architectural terms

- **Connector**: integrates an external source/sink.
- **Engine**: independent coherent runtime behind an Application; Buzz is the Chat Engine.
- **Contribution**: declarative shell/UI/search/settings metadata.
- **Extension**: future third-party sandboxed executable code.
- **Contract**: synchronous API schema or durable event schema.
- **action capability**: fine-grained authority such as `files.read` or `chat.post`.

## Application responsibilities

Every Application owns:

- domain model and business logic;
- private persistent state and migrations;
- authoritative resource-level authorization;
- resource identifiers/types;
- provided/required contracts;
- published/subscribed integration events;
- audit details;
- data export/deletion/retention semantics;
- declarative Contributions.

Applications must not read another Application's private tables.

## Platform Core responsibilities

Core owns only cross-product identity/foundation concerns:

- tenants/workspaces;
- principals and group membership;
- OIDC principal mapping;
- Application registry/enablement/configuration references;
- Application-level grants;
- service/workload identity;
- audit foundation;
- ResourceRef conventions/owner resolution;
- correlation/causation context.

Core does not own Application-specific business logic or Memory indexing/RAG.

## Initial first-party Applications

Required:

- Files
- Notes
- Mail
- Memory
- Chat
- Agents

Existing Decisions, Meetings, Standups and Kanban may initially remain embedded Application contributions/features. They should be promoted to separately owned Applications only when their domain/data/runtime requirements justify it. An Application boundary is not created merely to make a directory tree symmetrical.

Object Spaces is optional and deferred; it is not a dependency of the core product.

## Manifest

Each Application has one declarative `ApplicationManifest` following `docs/specs/application-manifest-v1alpha1.md`.

The manifest describes contributions and contracts. It does not dynamically load native code.

## Migration from Module

Because there is no backward-compatibility requirement, migration is a cutover rather than a compatibility layer.

Required:

1. Introduce the `Application` domain model and Application registry.
2. Migrate persisted `modules` records/settings to the new Application model.
3. Rename user-facing product terminology from Module to Application/App where appropriate.
4. Replace public Module API/routes with Application API/routes.
5. Remove legacy serde aliases and legacy Module compatibility tests once migrated.
6. Preserve user artifacts/content and stable resource IDs where meaningful.
7. Do not maintain two permanent registries or two permanent route families.

The current file-backed principles of ADR-0016 remain valid for Applications whose authoritative content is naturally file-backed. ADR-0016 no longer defines the platform's top-level modularity model.

## Consequences

### Positive

- Clear product/domain ownership.
- Runtime extraction can happen only when operationally valuable.
- Chat can be one native product experience while Buzz remains independent.
- External sources are modeled honestly as Connectors.
- No premature public plugin ABI.
- Existing module metadata can seed the manifest/contribution model.

### Negative

- One breaking migration of Module APIs/schema is required.
- Existing code paths relying on broad `AppState` need gradual dependency reduction.
- Contract discipline is required even while Applications share one process/database cluster.

## Rejected alternatives

### General-purpose plugin runtime now

Rejected. Dynamic native loading, WASM runtime, package management, dependency solving and marketplace concerns do not solve the immediate coupling problem.

### Microservices for every feature

Rejected. Deployment complexity is not modularity. Start with explicit boundaries in a modular monolith and extract only where lifecycle/scaling/failure isolation justifies it.

### Keep Module and add Plugin above it

Rejected. This preserves confused vocabulary and obsolete compatibility solely because it already exists.

## Acceptance criteria

- [ ] Canonical architecture uses Application terminology.
- [ ] Application manifest spec exists.
- [ ] Connector, Engine, Contribution and Extension are separately defined.
- [ ] One-time Module-to-Application migration is documented.
- [ ] No public third-party ABI/marketplace is introduced in the first implementation.
- [ ] Runtime strategy is not part of Application/resource identity.
