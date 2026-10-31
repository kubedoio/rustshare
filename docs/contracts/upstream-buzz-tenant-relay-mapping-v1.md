# Upstream Buzz Tenant-to-Relay Mapping Contract v1

**Status:** Proposed security contract; acceptance required before production use.

**Scope:** Elembra mapping policy for an unmodified upstream Buzz profile. This contract does not wire a connector, change production integration, or implement managed signing keys.

## Invariants

1. Each tenant has one isolated upstream relay deployment and one community. Every active workspace mapping for that tenant resolves to that same relay, community, relay identity, deployment provenance, and approved address set. A relay URL, host, or community cannot be reused by another tenant.
2. The trusted control plane owns immutable, audited mapping records keyed by `(tenant_id, workspace_id)`. A record contains a unique mapping ID, canonical `wss://` URL, exact DNS host, community ID, NIP-11 relay public key, deployment/source digest, operator-approved IP set, monotonically increasing generation, expiry, active state, updater, and append-only audit record ID.
3. User input never supplies or overrides a relay URL, host, community, IP, or relay key. Lookup must return exactly one active, current mapping for the authenticated Elembra tenant/workspace. Missing, duplicate, stale, conflicting, expired, or inactive records fail closed. There is no fallback to another tenant, an open relay, a legacy fork, or a previous mapping.
4. A mapping update uses compare-and-swap on the current generation, increments it exactly once, preserves mapping/tenant/workspace identity, and writes a new audit record. A stale update is rejected. Existing WebSocket sessions are scoped to `(tenant, workspace, mapping_id, generation)` and are never pooled or reused across scopes. A generation change invalidates an in-progress connection and requires affected sessions to be closed by the owning connector.
5. Resolve DNS once per connection attempt. Reject the connection if the answer set is empty, contains an internal/special-use address, or includes an address absent from the trusted approved set. The checked-in deny set covers the IANA IPv4 and IPv6 Special-Purpose Address Registries last updated 2025-10-09; it rejects every listed prefix, including globally reachable service ranges. It must be refreshed and reviewed against IANA before production integration. IPv6 is additionally accepted only from global-unicast space. Dial only a pinned answer while retaining the exact configured host for TLS SNI and certificate hostname verification. Disable redirects and proxy/environment overrides. The connector must verify the peer address and port after connect; it must not resolve the hostname again between validation and dial. Update approved DNS targets only through an audited mapping generation change.
6. Require a valid TLS chain and hostname match, the configured community, and the relay identity pinned from trusted deployment provenance. TLS errors, wrong host, wrong peer, wrong community, wrong relay key, DNS changes, and relay URL substitution fail closed. No transport error alone counts as an authorization denial.
7. Relay owner/admin signing credentials are separate from user signing identities. The user identity cannot create or change tenant mappings or obtain owner/admin privileges.

## Connection sequence

1. Authenticate the Elembra user and authorize the tenant/workspace before mapping lookup.
2. Select exactly one mapping from the trusted store. Validate generation, state, expiry, provenance digest, canonical URL/host, community, identity and approved IP set.
3. Resolve and validate DNS, create a scoped connection attempt carrying the immutable mapping snapshot, and dial a pinned IP with SNI set to the mapped host. Disable redirects and do not accept user-provided proxy settings.
4. Verify TLS chain and hostname, connected peer address/port, NIP-11 relay identity, community binding, and that the current trusted mapping still has the same generation.
5. Bind the WebSocket and subscriptions to the exact mapping scope. Re-check the current generation before operations that begin a new connection or resume a session. On mapping change, fail closed and close affected sessions; never silently reconnect through a different tenant mapping.

## Policy model and proof boundary

`backend/crates/resource-auth/src/upstream_relay_mapping.rs` is an isolated, executable policy model with negative tests for missing/ambiguous lookup, tenant/workspace mismatch, noncanonical or substituted URL/host, unsafe DNS answers, unpinned peers, TLS verification failure, wrong relay identity/community, stale or changed generation, duplicate/cross-tenant mappings, and audited compare-and-swap updates. It is deliberately not connected to the existing Buzz adapter or production WebSocket transport. The model's transport evidence fields are test inputs, not proof that the current connector enforces them. Production acceptance requires an implementation review and integration tests against the actual TLS/DNS connector, proxy policy, session pool and trusted mapping store.

## Audit and failure handling

Mapping writes record the actor, timestamp, previous and new generation, changed relay/provenance fields, approval/change reference, and result. Audit records are append-only and do not contain credentials. Read, DNS, TLS, identity, generation and scope failures are logged with mapping ID/generation and sanitized reason only. No private key, signed secret material, session token, or user message body is included. Ambiguous and missing mapping failures do not reveal another tenant's mapping details.

## Acceptance gates

Before enabling an upstream profile in production, independently verify trusted-store authorization, tenant/workspace selection, exact URL/host and relay identity pinning, TLS/SNI, DNS pinning and SSRF behavior, redirect/proxy behavior, websocket pool isolation, generation-change cancellation, audit immutability, failure paths, and relay key rotation. Required negative tests include mismatched tenant, wrong host, wrong relay identity, stale generation, update during connect, user URL substitution, internal/rebound DNS, duplicate mapping, and cross-tenant relay/community reuse. This contract does not weaken ADR-0034/0035/0036 or authorize the managed identity vault.
