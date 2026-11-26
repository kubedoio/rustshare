# H3 product decision records — updated 2026-11-26

Source snapshot: protected main `af8f695b66b14526be1eff0b027b353f807dce38`. The latest completed clean-install/browser candidate evidence is on former main `3bd949b6`; current-main CI and Pilot Release checks are pending. All candidate evidence remains pinned to its tested SHA. These decision records summarize current product scope and preserve open owner decisions as non-authorization.

## #325 Mail persistence

**Recommendation:** keep live, server-authorized Mail as the default and do not automatically archive or import every mailbox message. Offer a separate, explicit import/archive choice only after the owner selects retention and storage policy.

| Mode | Retention/storage | Privacy and Memory effect |
| --- | --- | --- |
| Live-only (recommended default) | Mail server remains authoritative; Elembra stores sync metadata and fetches authorized messages. | No second durable message copy in Files; revocation/deletion follows the current Mail account. Memory search remains live and does not create an independent Mail index. |
| Optional import/archive | Explicitly selected messages become Files artifacts with normal Files retention, quotas, backup and deletion controls. | Adds durable copies and storage/backup cost; source deletion does not erase an imported artifact. Memory can index only through the Files/ResourceRef path and must show the imported copy's ownership/retention semantics. |

Owner decision required: whether optional import/archive is in scope, which messages/attachments are eligible, and whether retention is user-selected or centrally bounded. Keep #325 open until those rules and acceptance are explicit.

## #329 Calendar interoperability

Current mainline has owner-scoped single-event export and bounded `.ics` import, but full interoperability is incomplete. Recurrence and timezone round-trip for supported cases and stable-UID duplicate behavior remain to be qualified. Permission-aware, revocable subscription/feed acceptance is deferred pending end-to-end secret-redaction evidence, and the CalDAV feasibility decision remains open. Unsupported recurrence overrides/custom zones must fail closed rather than silently change event meaning. Native Apple/Outlook/Google client acceptance and live Google/Microsoft provider sync (#507) remain separate gates requiring operator credentials.

**Recommendation:** retain the current bounded iCalendar contract for the pilot; defer full CalDAV and provider-specific live sync until named clients, recurring-event semantics, secret-redaction evidence and operator test accounts are available. Prior-head Pilot Release run [45756901843](https://github.com/kubedoio/rustshare/actions/runs/45756901843) passed on PR #580 head `f0ab4fd5`; the branch has since been refreshed onto main as `4ce142a570e010a87035deb360afeff19fd4131a`. Its new exact-head Pilot Release run [45763193953](https://github.com/kubedoio/rustshare/actions/runs/45763193953) and required PR checks are queued, so current-base Calendar acceptance is not yet established. Keep #329 and #507 open.

## #330 Organization administration

Mainline has organization-admin user/group workflows, audit paths, capability checks, and tenant-scoped group operations. Independent workspace membership and effective-role-path visibility are not implemented; ADR-0040 defers a second membership model. Existing resource authorization remains authoritative.

Prior PR #569 head `7d1926aee99f142bdb2f6c85fc994b8b6603e8c9` passed the approximately 100-user/two-admin Pilot Release journey in run [45753851378](https://github.com/kubedoio/rustshare/actions/runs/45753851378): 100 synthetic users, two admins performing independent actions with actor audit assertions, ordinary-user denial, pagination, search/offboarding and cleanup safeguards ([candidate evidence](../pilot/evidence/candidate-7d1926ae-pr569-disposable-pilot-2026-11-26.md)). The branch is now refreshed onto main as `95e72cff751b48a577ab93c506c7e38733da78e6`; exact-head checks and Pilot Release run [45763197651](https://github.com/kubedoio/rustshare/actions/runs/45763197651) are pending. The prior run establishes the UI scale subgate only, not current-head qualification. It does not establish independent administrator recovery, group-membership revocation through resource/search authorization, or FWS operational acceptance.

**Recommendation:** pilot only the current tenant-admin, group and ordinary-member boundaries. Do not add workspace membership or new role semantics without an owner decision on how they compose with resource permissions. Keep #330 open for the owner decision on workspace-membership scope and for the outstanding revocation/effective-role and target-environment acceptance; the disposable 100-user browser journey is recorded as passed, not as institutional operational acceptance.

## #355 Rebranding

Mainline supports deployment-wide build-time product name, same-origin logo path and a fixed palette set. Branding is shared across tenants; there is no runtime admin editor/persistence, and some legacy copy remains.

**Recommendation:** keep the pilot to deployment-wide branding. Defer tenant-specific themes, arbitrary CSS/HTML and runtime branding storage. Owner decision required on who controls future branding (deployment operator or tenant administrator) before persistence is designed. Keep #355 open.

## Long-term Chat authority

The evidence-based A–D comparison and recommendation are in [`chat-retain-migrate-drop.md`](../architecture/chat-retain-migrate-drop.md). ADR-0044 records the accepted Elembra target as **A: managed Elembra identities on unmodified upstream Buzz**; RustChat ADR-005 independently makes RustChat's own service authoritative with Buzz optional. The external RustChat repository already contains a substantial Rust/Axum and Vue chat product, but its org/team identity, Files storage, shared-key outbound Buzz bridge and history have not been qualified as an Elembra integration. The recommended operational posture is **D**: retain the supported fork-backed deployment with bounded B maintenance. Owner confirmation is requested on whether A remains the long-term Elembra target or whether C should receive a separate integration/migration evaluation. Keep #309's live deployment/data/backup inventory and #442, #458 and #460 independent gates. Do not migrate, enable managed publishing, change production identity mappings or remove legacy data.
