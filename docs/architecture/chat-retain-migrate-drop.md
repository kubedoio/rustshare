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

## Migration sequence if later authorized

1. Inventory each deployment and produce counts/checksums without changing data.
2. Define retained user-visible behavior and approved identity/workspace mapping.
3. Implement a disposable export/import proof; preserve event IDs, authorship and source references.
4. Restore and compare migrated history, attachments, authorization, search and Memory projections.
5. Rehearse per-tenant cutover and rollback with no production identities.
6. Obtain product-owner and independent security approval for one tenant at a time.
7. Keep the old authority read-only and recoverable through the agreed rollback window; retire only after acceptance.

Until then, retain existing functionality and data. Do not run two authoritative Chat backends, mutate production mappings, archive legacy data or implement H4 cutover code.

## Related records

- [Issue #309](https://github.com/kubedoio/rustshare/issues/309)
- [Issue #323](https://github.com/kubedoio/rustshare/issues/323)
- [`elembra-buzz-no-fork-migration-and-release-gates.md`](../plans/elembra-buzz-no-fork-migration-and-release-gates.md)
- [`elembra-chat-app-v1.md`](../implementation/elembra-chat-app-v1.md)
