# Chat retain, migrate, or drop inventory

This is a non-destructive planning record for #309. It does not authorize a cutover, identity rewrite, archive, export or deletion. Keep the currently deployed Buzz authority and its data until product owners accept parity, backup/restore and per-tenant cutover evidence.

## Current implementation inventory

The current Elembra Chat path uses the pinned Buzz relay as the authoritative event/history system. Elembra observes and projects supported events for application views, search, Memory references and audit. Browser-held identity remains a separate gated path; managed identity/publishing and legacy identity replacement have independent gates (#215, #323, #424, #442, #458, #460). Files attachments are references to Files resources and must be reopened through current Files authorization. Memory Chat records are derived projections, not a second authoritative message store.

Inventory each live deployment before a migration decision. Repository inspection alone cannot prove production state:

- [ ] Buzz source/revision, relay image digest, configuration, communities and external integrations.
- [ ] Elembra Chat APIs, browser workflows, permissions, channel membership, threading, replies, reactions, deletion/moderation and current error behavior.
- [ ] Tenant/workspace/community bindings, principals, browser identity keys, identity generations and any legacy mappings. Never modify production mappings during inventory.
- [ ] Event history: message IDs, authors, timestamps, signatures, references, edits/deletions and retention behavior.
- [ ] Attachments and Files ResourceRefs; verify object ownership and current reauthorization on open.
- [ ] Search and Memory projection schemas, outbox/worker consumers, cursors, receipts and reconciliation/rebuild procedures.
- [ ] API contracts, frontend routes, callbacks, webhooks, scheduled/background processes, secrets and backup/restore dependencies.
- [ ] Export available formats, completeness, event-signature preservation, encryption/secret boundaries and restore test status.

## Decision matrix

| Area | Decision | User-visible impact | Preservation/migration requirement | Validation and rollback | Gate |
| --- | --- | --- | --- | --- | --- |
| Chat UI and supported API workflows | Retain during H3 | No intentional behavior change | Capture route/API contract and current acceptance journeys | Browser/API regression on exact candidate | Current implementation stable |
| Buzz relay and event history | Retain as authority | Existing history and event IDs remain available | Back up relay/community state; prove export/import preserves event IDs, signatures and authors before any future move | Restore a copy; compare event counts/IDs; keep old relay available for rollback | No cutover until parity and restore are accepted |
| Browser-held identity | Retain, gated | Existing users continue their approved login/signing path | Preserve key custody and identity-to-principal mapping; no production rewrite | Disposable identity interoperability/recovery/revocation tests | #442 and related security gates |
| Managed identity/publishing | Defer | No new signing or identity replacement behavior | No new mappings or signer state | Upstream fixed revision and independent security proof | #215/#323/#424/#460 |
| Tenant/workspace/community mappings | Retain unchanged | Access must not broaden or shift | Export mapping inventory and owner-approved mapping specification | Cross-tenant denial and before/after mapping comparison; rollback from verified backup | Owner/security approval required |
| Files attachments | Retain references | Attachment opens continue through Files authorization | Preserve ResourceRefs and source mappings; never copy bytes without owner decision | Reauthorize open after revocation/deletion; compare references | Existing Files contract |
| Memory/Search projection | Retain as derived data; rebuild if needed | Search may be unavailable during rebuild, source history remains authoritative | Preserve consumer receipts/cursors; support rebuild from authorized source | Rebuild from source snapshot; compare scoped results; do not replay/delete receipts accidentally | Reconciliation evidence per source |
| Threads, replies, reactions, moderation | Retain behavior; map only after parity inventory | Do not silently drop features | Define mapping to official upstream primitives or explicitly retain current deployment | Feature-by-feature browser/API acceptance and rollback rehearsal | Upstream parity required |
| Legacy RustChat code/data | Unknown until deployment inventory; do not drop | Potential loss of legacy workflows/history | Identify any live database, object, identity or external-client dependency; export before proposing removal | Verify no active dependency plus restore/export test | Owner decision after inventory |
| Old relay/configuration/secrets | Retain until successful cutover and rollback window ends | Old authority remains recoverable | Store backup and secret-custody record under operator control | Restore rehearsal, then separately approved retirement plan | Never delete as part of inventory |

## A-D comparison by required capability

| Capability | A. Unmodified upstream Buzz | B. Existing fork-backed Buzz | C. Native authoritative RustChat | D. Keep current deployment and defer |
| --- | --- | --- | --- | --- |
| Enterprise identity and SSO | Existing upstream identity path; Elembra identity/revocation parity is not established. | Current Elembra principal/workspace bindings and gated browser-held keys; managed identity/publishing remains disabled. | No implementation; would need SSO, principal mapping, key custody and revocation. | Keeps current identity flow and mappings unchanged. |
| Tenant/workspace isolation and channel membership | Fork-specific signed authorization/state APIs used by Elembra are absent; adapter and cross-tenant proof required. | Existing tenant/community bindings and live channel-membership reauthorization; keep source authority in Buzz. | No implementation; tenant, workspace and channel ACLs must be built and independently tested. | Preserves the currently supported boundary and defers a second authority. |
| Messaging, threads, replies, reactions and moderation | Feature-by-feature parity is unproven; current fork behavior may not map directly. | Current supported behavior is retained; deletion/moderation semantics stay with Buzz. | All real-time messaging and feature parity must be implemented. | No user-visible feature change; no cutover. |
| History and search | History import/export parity, signed provenance, and rollback are unproven. | Buzz remains the signed history authority; Elembra search/Memory are derived projections with replay/reconciliation. | Must build authoritative history, search, retention and recovery. | Retains history authority and current derived projections. |
| Attachments and Files authorization | Must preserve Files ResourceRefs and prove current authorization on every open. | Attachments remain Files references and are reauthorized by Files. | Must build Files integration and revocation-aware open behavior. | Existing Files contract remains in force. |
| Cryptographic security and key custody | Desktop/keychain interoperability and independent acceptance remain open (#442, #458); upstream API differences need review. | Signed flow exists; browser-held keys remain gated and managed signing is off. Fork fixes must be tracked. | Must design and qualify signing, custody, rotation, revocation and client compatibility. | Keeps current custody; does not activate managed publishing. |
| Recovery, offboarding and external desktop clients | Recovery/revocation and unmodified desktop interoperability are not qualified. | Existing replay/reconciliation paths remain; independent recovery, offboarding and desktop gates are still required. | Must provide export/restore, offboarding, revocation and external client interoperability. | Defers cutover while retaining existing data and mappings. |
| Upstream and dependency risk | Lowest downstream fork burden, but required APIs/features depend on upstream acceptance and response. | Ongoing rebase, vulnerability-fix and image-release burden; responsiveness is limited. | No upstream Chat dependency, but all application and protocol maintenance becomes ours. | Accepts B's bounded maintenance burden while gates are closed. |
| Operations and engineering cost | Requires adapter, parity work, data validation and rollback; medium/high transition cost. | Lowest transition cost; recurring fork maintenance and security tracking remain. | Highest build and ongoing maintenance cost. | Lowest immediate change; does not erase B's maintenance cost. |
| Migration and rollback | Keep old authority, prove history/identity/attachment parity, then rehearse cutover and rollback. | No migration required to continue the current service. | Major migration, data conversion and rollback evidence required. | No migration, publication change, mapping edit or legacy-data removal authorized. |

## Migration sequence if later authorized

1. Inventory each deployment and produce counts/checksums without changing data.
2. Define retained user-visible behavior and approved identity/workspace mapping.
3. Implement a disposable export/import proof; preserve event IDs, authorship and source references.
4. Restore and compare migrated history, attachments, authorization, search and Memory projections.
5. Rehearse per-tenant cutover and rollback with no production identities.
6. Obtain product-owner and independent security approval for one tenant at a time.
7. Keep the old authority read-only and recoverable through the agreed rollback window; retire only after acceptance.

Until then, retain existing functionality and data. Do not run two authoritative Chat backends, mutate production mappings, archive legacy data or implement H4 cutover code.

## H4 architecture recommendation (2026-11-26)

Architecture source snapshot: protected main `a4d45723f6d63b4b478ce08b449e03354cb26e49` (PR #573 added a Local Text integration-test synchronization fix). The tracked Chat application uses Buzz as its signed message/history authority and depends on fork-specific signed authorization, state and channel APIs. `config/buzz-compatibility.env` pins Buzz source `b3aaabf635d992b08b57773071686a92196884e1` and image `ghcr.io/kubedoio/buzz-elembra@sha256:38c362bb37d15ea879634b0f56a867401df4e4e0074a84382dff6b07bb578b4e`; `docker-compose.alpha.yml` selects `RUSTSHARE_CHAT_AUTHORITY=buzz`. The web client in `frontend/src/lib/chat/` implements the current fork's NIP-29 channel, NIP-10 thread, NIP-25 reaction and deletion/moderation flows. `backend/server/src/buzz_gateway.rs` uses signed NIP-98 requests and pinned signed relay responses for access/state operations. Current-main searches show no RustChat authority service; ADR-0034 records earlier client/integration work, but #309 still requires deployment-level inventory of any live RustChat data, identities, external clients and backup dependencies. Repository absence is not evidence that deployed legacy data is absent.

ADR-0044 already records the owner's strategic choice of **A: managed Elembra identities on unmodified upstream Buzz** and authorizes isolated Phase 2A planning. It explicitly does not authorize security-gate completion, managed Chat enablement, release or migration. The actual supported deployment remains the fork-backed configuration (B) while the upstream target is qualified. Therefore the present operational posture is D: preserve the deployed system and defer cutover; that is not a request to replace the owner's selected target.

| Dimension | A. Unmodified upstream Buzz | B. Existing fork-backed Buzz | C. Native authoritative RustChat | D. Retain deployment and defer cutover |
| --- | --- | --- | --- | --- |
| Enterprise identity and SSO | Owner-selected managed-identity target; provisioning, key custody and export remain gated by ADR-0044/0045 and #215/#323/#424. | Current Elembra principal/workspace binding works with the pinned relay; current browser-held identity path remains distinct from managed identities. | No separate implementation is tracked; identity, SSO, provisioning and recovery would need building. | Preserve current identity mappings; no production rewrite. |
| Tenant, workspace and channel authorization | Stock Buzz does not provide the fork's Elembra signed access/state/channel APIs; an adapter plus live-membership tests is required. | Existing signed flow and live membership reauthorization are in the supported adapter; preserve tenant/community mappings. | Must implement tenant/workspace isolation, channel ACLs and revocation from scratch. | Keep current checks and mappings unchanged while parity is proven. |
| Messaging, threads, moderation | Desktop and relay feature parity is not established for the required Elembra workflows; #442/#458 remain gates. | Current deployment supports the integrated channel/message paths; preserve thread, reply, reaction and moderation behavior. | Must implement realtime delivery, history, threads, replies, reactions and moderation. | No feature switch or second authoritative backend. |
| History and search | Buzz events remain authoritative; signed IDs/authors, edits/deletions, search and Memory projection need export/import and parity proof. | Existing Buzz event history feeds Elembra views/search and derived Memory references. | Requires a new authoritative store, search and data conversion. | Keep the current event store; no migration or legacy-data deletion. |
| Attachments and Files | Adapter must retain Files ResourceRefs and reauthorize each open against current Files permissions. | Attachments are Files references and are reopened through Files authorization. | Must integrate Files ownership, storage and authorization. | Keep the current Files boundary. |
| Cryptography and key custody | Matches the accepted product target, but key wrapping, reveal/export, revocation and independent review remain gated; no signer is enabled. | Retains the existing fork and browser-key paths; carries downstream cryptographic review and patch obligations. | Must design and independently review protocols, keys and custody. | No new signer, publishing mode or identity mapping. |
| Recovery, offboarding and desktop interoperability | Must prove key recovery/offboarding, NIP-29 history parity and stock desktop acceptance (#442); these are not proven by source inspection. | Current history can remain on the pinned relay; preserve existing recovery and offboarding behavior. | Must build and qualify recovery, offboarding and external-client contracts. | Avoids cutover risk while retaining fork recovery obligations. |
| Upstream and dependency risk | Removes long-term fork carry only after upstream compatibility; upstream cadence and adapter/API change risk remain. | Requires tracking upstream fixes, maintaining a downstream patch set and immutable reviewed pins. | Owns all product and protocol maintenance. | Lowest immediate change; fork vulnerability and rebase work continues. |
| Operations and cost | Medium/high transition cost: adapter, managed identity, parity, backup and rollback qualification. | Lowest transition cost; recurring fork security and release maintenance. | Highest implementation and long-term maintenance cost. | Lowest immediate operational change; no cost reduction while the fork remains. |
| Migration and rollback | Requires signed-history/identity/attachment parity, per-tenant migration, fresh restore and rollback evidence. | No migration is needed to continue current service. | Requires major data conversion, identity mapping and rollback proof. | Do not cut over, rewrite mappings or remove legacy data until authorized and proven. |

**Recommendation and decision status:** keep D operationally with bounded B maintenance while preserving the owner's previously selected A strategic target. B maintenance is limited to security and compatibility fixes, pinned artifacts and regression/conformance checks; avoid adding new fork-only product surface without an owner decision. A's managed identity target remains gated for implementation, enablement and release by its ADRs, #442 desktop acceptance, #458 independent security acceptance, #460 publishing gate and #309 deployment/data inventory. C is not recommended on current evidence: no native RustChat authority implementation or approved migration plan is tracked. Owner confirmation is requested for D as the interim operational posture and for the cutover evidence/timing; no migration, production mapping change, managed publishing or legacy data removal is authorized here. Chat migration remains **BLOCKED**.

## Related records

- [Issue #309](https://github.com/kubedoio/rustshare/issues/309)
- [Issue #323](https://github.com/kubedoio/rustshare/issues/323)
- [`elembra-buzz-no-fork-migration-and-release-gates.md`](../plans/elembra-buzz-no-fork-migration-and-release-gates.md)
- [`elembra-chat-app-v1.md`](../implementation/elembra-chat-app-v1.md)
