# H3 product decision records — 2026-11-25

These records use protected main `267b9f14dc180af79b1cf0522949f1159258ffc9` as their product/source snapshot, including the bounded organization group-member removal changes in PR #560. Protected main has since advanced to `91ac914a8bc011fb59a723f7f253adbfdc213494` through dependency-only PR #547; the issue scope and product contracts summarized here are unchanged. Recommendations preserve the current supported pilot scope; pending owner choices are not implementation authorization.

## #325 Mail persistence

**Recommendation:** keep live, server-authorized Mail as the default and do not automatically archive or import every mailbox message. Offer a separate, explicit import/archive choice only after the owner selects retention and storage policy.

| Mode | Retention/storage | Privacy and Memory effect |
| --- | --- | --- |
| Live-only (recommended default) | Mail server remains authoritative; Elembra stores sync metadata and fetches authorized messages. | No second durable message copy in Files; revocation/deletion follows the current Mail account. Memory search remains live and does not create an independent Mail index. |
| Optional import/archive | Explicitly selected messages become Files artifacts with normal Files retention, quotas, backup and deletion controls. | Adds durable copies and storage/backup cost; source deletion does not erase an imported artifact. Memory can index only through the Files/ResourceRef path and must show the imported copy's ownership/retention semantics. |

Owner decision required: whether optional import/archive is in scope, which messages/attachments are eligible, and whether retention is user-selected or centrally bounded. Keep #325 open until those rules and acceptance are explicit.

## #329 Calendar interoperability

Current mainline has owner-scoped single-event export and bounded `.ics` import, but full interoperability is incomplete. Recurrence and timezone round-trip for supported cases and stable-UID duplicate behavior remain to be qualified. Permission-aware, revocable subscription/feed acceptance is deferred pending end-to-end secret-redaction evidence, and the CalDAV feasibility decision remains open. Unsupported recurrence overrides/custom zones must fail closed rather than silently change event meaning. Native Apple/Outlook/Google client acceptance and live Google/Microsoft provider sync (#507) remain separate gates requiring operator credentials.

**Recommendation:** retain the current bounded iCalendar contract for the pilot; defer full CalDAV and provider-specific live sync until named clients, recurring-event semantics, secret-redaction evidence and operator test accounts are available. Keep #329 and #507 open.

## #330 Organization administration

Mainline has organization-admin user/group workflows, audit paths, capability checks, and tenant-scoped group operations. Independent workspace membership and effective-role-path visibility are not implemented; ADR-0040 defers a second membership model. Existing resource authorization remains authoritative.

**Recommendation:** pilot only the current tenant-admin, group and ordinary-member boundaries. Do not add workspace membership or new role semantics without an owner decision on how they compose with resource permissions. Keep #330 open until the membership scope and approximately 100-user operational acceptance are demonstrated.

## #355 Rebranding

Mainline supports deployment-wide build-time product name, same-origin logo path and a fixed palette set. Branding is shared across tenants; there is no runtime admin editor/persistence, and some legacy copy remains.

**Recommendation:** keep the pilot to deployment-wide branding. Defer tenant-specific themes, arbitrary CSS/HTML and runtime branding storage. Owner decision required on who controls future branding (deployment operator or tenant administrator) before persistence is designed. Keep #355 open.

## Long-term Chat authority

The evidence-based A–D comparison and recommendation are in [`chat-retain-migrate-drop.md`](../architecture/chat-retain-migrate-drop.md). ADR-0044 records the owner-selected strategic target as **A: managed Elembra identities on unmodified upstream Buzz**. The current operational posture is **D**: retain the supported fork-backed deployment with bounded B maintenance until the deployment/data inventory and independent gates pass. Confirm only interim retention and cutover timing under the existing A target; this record does not ask the owner to select a new long-term architecture. Do not migrate, enable managed publishing, change production identity mappings or remove legacy data. #309 inventory and #442, #458 and #460 remain independent gates.
