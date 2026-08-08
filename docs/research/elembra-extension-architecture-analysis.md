# Elembra extension-architecture analysis

Status: Architecture research  
Date: 2026-08-07  
Decision target: Elembra Application architecture

## Purpose

Elembra is not required to preserve the current RustShare module API, internal service graph, route names, or a public third-party plugin ABI. The product is still pre-release and we control the first-party Applications. This gives us an unusual opportunity: use mature extension systems as evidence, but do not copy the compatibility machinery they accumulated over years.

This document compares relevant architectures and records what Elembra should adopt, reject, or defer.

## Systems reviewed

### Visual Studio Code

Official references:

- <https://code.visualstudio.com/api/advanced-topics/extension-host>
- <https://code.visualstudio.com/api/references/contribution-points>

Useful ideas:

- Declarative contribution metadata is separate from executing extension code.
- Extension code runs outside the UI process in an extension host.
- Extensions contribute commands, views, menus and other surfaces through a manifest instead of directly mutating the host UI.
- Isolation is used so an extension cannot casually block or corrupt the core UI.

Elembra lesson:

- The Application manifest should be declarative.
- UI navigation, routes, commands, dashboard cards, settings and search providers should be declared as contributions.
- A manifest is not a code-loading mechanism.

What not to copy yet:

- A public extension marketplace.
- Multiple extension-host runtimes.
- A compatibility promise for arbitrary third-party extensions.

### Terraform providers

Official references:

- <https://developer.hashicorp.com/terraform/plugin>
- <https://developer.hashicorp.com/terraform/plugin/how-terraform-works>
- <https://developer.hashicorp.com/terraform/plugin/framework/provider-servers>

Useful ideas:

- Core and domain-specific implementations are separated.
- Providers are independently executable processes and communicate through an explicit RPC boundary.
- Provider implementations own the semantics of their external domain.
- Terraform explicitly discourages importing provider internals as libraries; the supported boundary is the protocol.

Elembra lesson:

- Service-backed Applications and Connectors must expose stable contracts rather than sharing implementation packages or database tables.
- A domain with a coherent runtime should remain independently owned.
- Buzz therefore belongs behind the Elembra Chat Application as an external Engine, not as code imported into Elembra Core.

What not to copy yet:

- Terraform's multi-version plugin protocol and handshake machinery. Those exist because Terraform and providers have long independent release histories. Elembra owns its first-party components and can start with one `v1alpha1` contract generation.

### Kubernetes API extension and controller model

Official references:

- <https://kubernetes.io/docs/concepts/extend-kubernetes/api-extension/custom-resources/>
- <https://kubernetes.io/docs/concepts/architecture/controller/>

Useful ideas:

- Extend through APIs and controllers instead of private database coupling.
- Reconciliation and eventual consistency are normal distributed-system tools.
- Kubernetes documentation explicitly recommends a standalone service when an existing program already serves a coherent API well.
- Kubernetes warns against using its extension API as storage for ordinary application/end-user data.

Elembra lesson:

- Application-owned data stays with the owning Application.
- Cross-Application integration is references + APIs + durable events, not shared tables.
- Buzz remains the source of truth for chat.
- Google Drive, Dropbox and OneDrive remain external sources with their own semantics rather than pretending to be identical Elembra storage backends.

### Backstage backend system

Official references:

- <https://backstage.io/docs/backend-system/>
- <https://backstage.io/docs/backend-system/architecture/index/>
- <https://backstage.io/docs/backend-system/architecture/extension-points/>

Useful ideas:

- Distinguish top-level plugins/features from modules that extend one feature.
- Keep extension-point interfaces small.
- The unit of deployment can differ from the logical feature boundary.

Elembra lesson:

- `Application` is the logical/product boundary; `embedded`, `service`, and `bridge` are runtime strategies, not different product concepts.
- Fine-grained interfaces should be small and owned by the Application exposing them.

What not to copy yet:

- A generalized dependency-injection/service-locator framework for third parties. Compile-time Rust composition remains preferable for embedded first-party Applications.

### OpenTelemetry Collector

Official references:

- <https://opentelemetry.io/docs/collector/architecture/>
- <https://opentelemetry.io/docs/collector/extend/>

Useful ideas:

- A precise component taxonomy is better than calling every extensibility mechanism a plugin.
- Receivers, processors, exporters, connectors and extensions have different responsibilities.

Elembra lesson:

Use distinct terms:

- **Application** — first-party product/domain boundary visible in Elembra.
- **Connector** — integration with an external source or sink.
- **Engine** — independently coherent runtime used behind an Application, such as Buzz behind Elembra Chat.
- **Contribution** — declarative shell/UI/search/settings contribution.
- **Extension** — future sandboxed third-party code.
- **Contract** — typed synchronous API or durable event schema.

### CloudEvents

Official reference:

- <https://cloudevents.io/>

Useful ideas:

- Common event metadata across unrelated producers.
- Event identity and source are explicit.
- Standard envelopes make routing, tracing and tooling easier.

Elembra lesson:

Cross-Application integration events should use a CloudEvents 1.0-compatible envelope, extended with Elembra tenant/workspace/principal/resource/correlation metadata. Application event types are namespaced strings rather than variants in one central Rust enum.

### WebAssembly Component Model and Extism

Official references:

- <https://component-model.bytecodealliance.org/design/wit.html>
- <https://extism.org/docs/concepts/plug-in-system/>
- <https://extism.org/docs/concepts/host-functions/>

Useful ideas:

- WIT defines language-neutral contracts.
- WebAssembly provides a useful sandbox for code that should not receive ambient host access.
- Host functions can expose a deliberately small authority surface.

Elembra lesson:

WASM/WASI or Extism is a good future option when Elembra accepts untrusted third-party Extensions.

What not to do now:

- Do not make WASM the architecture of Files, Notes, Mail, Memory, Agents or Chat.
- Do not freeze a public ABI before the first-party Application contracts have survived real use.

## Current RustShare findings

The existing code already contains the beginnings of an Application manifest:

- `backend/crates/core/src/domain/module.rs` stores module identity, UI configuration, permissions, AI indexing and audit configuration.
- `backend/server/src/services/module_service.rs` contributes navigation, dashboard behavior, renderers and templates.

But the runtime boundary is not real:

- `backend/server/src/state.rs` wires domain services directly into `ServiceState` and `AppState`.
- Services can therefore accumulate hidden dependencies on the entire application graph.

The current event path also has two different concerns mixed together:

- the `events` table can persist domain events;
- `EventBroadcaster` is an in-memory Tokio broadcast channel intended for live WebSocket fan-out and can lag/drop events for consumers.

`EventBroadcaster` must therefore never become the cross-Application integration bus.

The existing `ChatIntegrationService` is a valuable early seam: it already models signed outbound webhooks, inbound integration events and permission-aware unfurls. It should be generalized/evolved, not replaced by shared Buzz/Elembra database access.

## Compatibility freedom

Unlike the mature systems above, Elembra does **not** currently need to support an installed ecosystem of independently versioned third-party plugins.

Therefore the first architecture should deliberately avoid:

- legacy `Module` aliases in the new Application API;
- simultaneous `/modules` and `/applications` public contracts indefinitely;
- multiple Application-manifest versions;
- RPC protocol negotiation;
- hot loading/unloading of native Rust libraries;
- a plugin package manager;
- a marketplace;
- public ABI stability guarantees.

Instead:

1. Define one target architecture.
2. Migrate persisted user data once.
3. Rename `Module` to `Application` directly in the public product/API model.
4. Use `v1alpha1` for contracts while first-party Applications are being proven.
5. Break `v1alpha1` deliberately when a cleaner design is discovered.
6. Declare a stable public extension contract only after Files, Memory, Connectors and Chat have exercised the same primitives.

## Decision matrix

| Pattern | Elembra decision | Reason |
|---|---|---|
| Declarative contribution manifest | Adopt now | Existing module metadata already provides a migration path |
| Compile-time embedded first-party Applications | Adopt now | Lowest operational complexity while boundaries mature |
| Service-backed Applications | Support as a runtime strategy | Needed for Memory workers, Agents and independently scaling domains |
| Bridge-backed Application over external Engine | Adopt now | Correct model for Buzz/Elembra Chat |
| External Connectors | Adopt now | Correct model for Drive/Dropbox/OneDrive/shell/editor sources |
| Durable event outbox | Adopt before service extraction | Prevents lost cross-process integration events |
| CloudEvents-compatible envelope | Adopt now | Neutral event metadata without inventing a proprietary envelope |
| Shared cross-Application database access | Reject | Destroys ownership, authorization and independent evolution |
| Transparent Drive/Dropbox/S3 `ArtifactStore` abstraction | Reject | Semantics are materially different and the abstraction leaks |
| Runtime native-library plugins | Reject | Unsafe ABI and process stability cost with no current need |
| WASM third-party Extensions | Defer | Valuable only after a real third-party extension requirement exists |
| Marketplace/public SDK | Defer | Would freeze immature contracts |
| Kafka/NATS as prerequisite | Reject | Postgres outbox is enough initially; transport remains replaceable |
| gRPC as mandatory Application protocol | Reject for now | HTTP/JSON + typed schemas is simpler for first-party mixed clients; use gRPC only for a proven need |

## Resulting architectural principle

> Elembra is a platform of Applications. Applications own domains and data. Connectors integrate external systems. Engines remain independently coherent runtimes behind Applications. Contributions compose the user experience. Contracts and durable events connect boundaries. Third-party executable Extensions come later, after the first-party architecture is proven.
