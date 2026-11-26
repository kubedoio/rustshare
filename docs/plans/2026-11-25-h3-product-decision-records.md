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

**Strategic architecture (already approved):** ADR-0044 records the project owner's approval of **A: managed Elembra identities on unmodified upstream Buzz** as the target architecture. ADR-0046 records the same owner-selected Option A for managed NIP-29 publishing. These are product/architecture decisions only; they do not grant independent security acceptance or authorize real-user key custody, signing, publication, release, or migration. No additional owner choice between A and D is pending.

**Current deployment posture: D, with bounded B maintenance.** Preserve the supported fork-backed deployment while the upstream profile gates remain open. Protected main `af8f695b66b14526be1eff0b027b353f807dce38` pins the current Buzz source `b3aaabf635d992b08b57773071686a92196884e1` and image `ghcr.io/kubedoio/buzz-elembra@sha256:38c362bb37d15ea879634b0f56a867401df4e4e0074a84382dff6b07bb578b4e` in `config/buzz-compatibility.env`; `docker-compose.alpha.yml` selects `RUSTSHARE_CHAT_AUTHORITY=buzz`. Keep B maintenance limited to security/compatibility fixes, reviewed immutable pins, and regression/conformance checks. Avoid new fork-only product surface.

**Candidate evidence:** PR #567 Pilot Release run [45757287608](https://github.com/kubedoio/rustshare/actions/runs/45757287608) passed on tested source head `e8f65f909cf78ecd2509c6e23f4bcf72bef41342`; its tree matches protected main `af8f695b`. It qualifies clean install, migrations, browser pilot workflows, upgrade, backup/restore and rollback; it is not Buzz protocol parity, native desktop interoperability, or Chat migration evidence. See the [candidate-specific evidence bundle](../pilot/evidence/candidate-e8f65f90-pr567-disposable-upgrade-2026-11-26.md). The exact protected-main Pilot Release run [45762571913](https://github.com/kubedoio/rustshare/actions/runs/45762571913) is currently in progress; do not claim that run as passed.

**Why not C now:** RustChat is a substantial separate product and a real alternative, but its own organization/team authority, file storage, message history and outbound shared-key Buzz bridge are not a qualified Elembra integration. Its data/identity deployment inventory, Files/Memory authorization mapping, source provenance and rollback remain unproven. Do not start a parallel authoritative Chat deployment or migration. C would need a separately scoped evaluation only if the owner explicitly reopens the accepted A target.

**Independent gates remain:** #458's managed-history Cilium execution passed in run `41936521775`, but independent security acceptance is pending; #442's interactive unmodified Buzz Desktop/native-keychain acceptance is unverified; #460 remains blocked on an official upstream membership-cache/role-transition fix and separate publisher acceptance. #215, #323 and #424 keep managed custody, signing, export, and legacy identity replacement gated. Keep #309 open for the live deployment/data/backup inventory and tested export/migration/rollback requirements.

**Recommendation:** A remains the strategic target; D is the current operational posture; B is bounded maintenance; C is not recommended on current evidence. Preserve current mappings and data. This record does not authorize implementation, managed identity/signing, publishing, deployment, migration, or data removal.
