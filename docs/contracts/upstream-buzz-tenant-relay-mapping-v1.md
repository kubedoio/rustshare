# Upstream Buzz Tenant-to-Relay Mapping Contract v1

**Status:** Target architecture accepted for planning; independent security acceptance and production implementation are pending.

**Scope:** Elembra mapping policy for an unmodified upstream Buzz profile. This contract does not wire a connector, change production integration, or implement managed signing keys.

## Invariants

1. Each tenant has one isolated upstream relay deployment and one community. Every active workspace mapping for that tenant resolves to that same relay, community, relay identity, deployment provenance, and approved address set. A relay URL, host, or community cannot be reused by another tenant.
2. The trusted control plane owns immutable, audited mapping records keyed by `(tenant_id, workspace_id)`. A record contains a unique mapping ID, endpoint policy (`strict_public` or `trusted_service`), canonical `wss://` URL, exact DNS host and port, community ID, relay public key, deployment/source digest, approved address/service identity, monotonically increasing generation, expiry, active state, updater, and append-only audit record ID.
3. User input never supplies or overrides a relay URL, host, community, IP, or relay key. Lookup must return exactly one active, current mapping for the authenticated Elembra tenant/workspace. Missing, duplicate, stale, conflicting, expired, or inactive records fail closed. There is no fallback to another tenant, an open relay, a legacy fork, or a previous mapping.
4. A mapping update uses compare-and-swap on the current generation, increments it exactly once, preserves mapping/tenant/workspace identity, and writes a new audit record. A stale update is rejected. Existing WebSocket sessions are scoped to `(tenant, workspace, mapping_id, generation)` and are never pooled or reused across scopes. A generation change invalidates an in-progress connection and requires affected sessions to be closed by the owning connector.
5. `strict_public` is the default endpoint policy. Resolve DNS once per connection attempt; reject empty answers, internal/special-use addresses and any unapproved result. IPv6 is additionally restricted to global unicast. Refresh the checked-in IANA deny set and review it before production integration. Dial only a validated pinned address while retaining the configured host for TLS SNI and hostname verification. Disable redirects and proxy/environment overrides, and verify the actual peer address and port after connect.
6. A `trusted_service` private/bundled relay exception is a distinct policy, not a general private-address allowlist. It remains disabled until independently security-reviewed and network-enforced. Each mapping must bind an immutable orchestrator/service identity (cluster/project, namespace, service UID or equivalent), admin-approved exact DNS service name and port, community, relay key/provenance, TLS hostname and peer/SPKI identity, and a narrowly scoped destination allowlist. The trusted resolver must return only addresses assigned to that service; the connector pins the selected address and verifies the peer without a second lookup. Network egress controls must deny every destination except the approved service. Private-range, loopback, link-local, metadata, Unix-socket and user-supplied destinations remain denied unless that exact service identity and address are authorized through this exception. No plain `ws://`, broad RFC1918 range or proxy route is allowed.
7. Require a valid TLS chain and hostname match, the configured community, and the relay identity pinned from trusted deployment provenance under either policy. TLS errors, wrong host, wrong peer, wrong community, wrong relay key, DNS changes and relay URL substitution fail closed. NIP-11 metadata is not itself a trust anchor. No transport error alone counts as an authorization denial.
8. Relay owner/admin signing credentials are separate from user signing identities. The user identity cannot create or change tenant mappings or obtain owner/admin privileges.

## Connection sequence

1. Authenticate the Elembra user and authorize the tenant/workspace before mapping lookup.
2. Select exactly one mapping from the trusted store. Validate generation, state, expiry, provenance digest, canonical URL/host, community, identity and approved IP set.
3. Resolve and validate DNS, create a scoped connection attempt carrying the immutable mapping snapshot, and dial a pinned IP with SNI set to the mapped host. Disable redirects and do not accept user-provided proxy settings.
4. Verify TLS chain and hostname, connected peer address/port, relay identity from trusted deployment provenance, community binding, and that the current trusted mapping still has the same generation. NIP-11 metadata is not itself a trust anchor.
5. Bind the WebSocket and subscriptions to the exact mapping scope. Re-check the current generation before operations that begin a new connection or resume a session. On mapping change, fail closed and close affected sessions; never silently reconnect through a different tenant mapping.

## Policy model and proof boundary

`backend/crates/resource-auth/src/upstream_relay_mapping.rs` is an isolated policy model whose current tests implement the strict-public default and negative cases. It does not yet implement or authorize the private `trusted_service` exception and is not connected to the production WebSocket transport. Model transport fields are test inputs, not proof of connector enforcement. Before enabling the exception, independently review its threat model and implement egress enforcement plus integration tests against the real resolver, TLS connector, proxy policy, session pool and trusted mapping store. Do not weaken the current internal/special-use rejection before those controls exist.

## Audit and failure handling

Mapping writes record the actor, timestamp, previous and new generation, changed relay/provenance fields, approval/change reference, and result. Audit records are append-only and do not contain credentials. Read, DNS, TLS, identity, generation and scope failures are logged with mapping ID/generation and sanitized reason only. No private key, signed secret material, session token, or user message body is included. Ambiguous and missing mapping failures do not reveal another tenant's mapping details.

## Acceptance gates

Before enabling an upstream profile in production, independently verify trusted-store authorization, tenant/workspace selection, exact URL/host and relay identity pinning, TLS/SNI, DNS pinning and SSRF behavior, redirect/proxy behavior, websocket pool isolation, generation-change cancellation, audit immutability, failure paths, and relay key rotation. Required negative tests include mismatched tenant, wrong host, wrong relay identity, stale generation, update during connect, user URL substitution, internal/rebound DNS, duplicate mapping, and cross-tenant relay/community reuse. The trusted-service exception needs its own independent threat-model review and network-egress proof. This contract records accepted architecture, not production enforcement or independent security approval.
