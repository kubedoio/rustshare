# ADR-0032: Mail Module Boundaries

Status: Proposed
Date: 2026-07-07
Owner: RustShare Core Team
Related: Epic-0147, Spec-Mail-Module, ADR-0016, ADR-0017, ADR-0018, ADR-0020, ADR-0021, ADR-0025, ADR-0031

## Context

RustShare issue #147 asks for mail integration. Mail is a high-value source of operational decisions, project history, and external context: users currently copy and paste mail into notes, tasks, or meeting records by hand. RustShare needs a safe, permission-aware way to import and reference mail without becoming an uncontrolled mailbox mirror or a full webmail clone.

The existing RustShare architecture provides clear patterns and boundaries for this work:

- Module/template/instance/renderer pattern ([ADR-0016](0016-file-backed-template-modules.md), [ADR-0017](0017-template-registry-and-admin-governance.md), [ADR-0018](0018-webui-module-navigation-and-dashboard-integration.md))
- File-backed attachments with portable identity ([ADR-0021](0021-file-backed-attachments-and-portability.md))
- Object-store-centered storage layout ([ADR-0001](0001-ADR.md), [ADR-0025](0025-storage-layout-and-file-identity.md))
- Permission-aware RAG and AI boundaries ([ADR-0020](0020-okf-notes-reconciliation-and-rag-safety.md))
- Tenant isolation, share links, and access control ([ADR-0031](0031-tenant-isolation-share-links-and-rls.md))
- Security model and agent safety boundaries ([`docs/security-model.md`](../security-model.md), [`docs/agent-guides/safety-boundaries.md`](../agent-guides/safety-boundaries.md))

## Decision

1. **RustShare Mail is a module inside RustShare**, not a standalone product or a full Gmail/Outlook/Exchange replacement. It extends workspace productivity; it does not compete as a general-purpose webmail suite.

2. **Two concepts are kept distinct:** a *live mailbox connection* and an *imported RustShare mail artifact*. The first version prioritizes selected import and archive workflows over live webmail behavior.

3. **Imported mail becomes a RustShare artifact.** It carries metadata, permissions, an audit trail, and optional links to notes, meetings, Kanban cards, or files. The artifact is the unit of sharing and reference inside RustShare.

4. **Storage follows the existing RustShare direction.** Mail bodies, attachments, and the original RFC 822/MIME source live in S3-compatible/RustFS object storage. Metadata lives in PostgreSQL during the Public Preview and migrates toward object-storage sidecars as the zero-PostgreSQL architecture matures.

5. **Default permission is private to the importing user.** Any workspace or project sharing must be explicit and follows RustShare's existing permission model.

6. **Linking mail to a note, Kanban card, or meeting does not automatically expose private mail content.** Links are references; visibility of the mail artifact itself remains governed by its own permissions.

7. **IMAP is a source/import mechanism only in early phases.** SMTP and outbound sending are out of scope for the first versions.

8. **AI/RAG indexing of mail is opt-in and policy-driven**, and it is permission-enforced. Live mailbox content is never sent to AI by default.

9. **The Mail module UI may be inspired by traditional webmail clarity** for readability and navigation, but it is not a Roundcube clone and does not pretend to be a standalone webmail product.

10. **Security controls apply from the start:** encrypted secrets, no plaintext passwords, audit events for sensitive actions, HTML sanitization for rendered bodies, attachment scanning hooks, and size/rate limits on import and display.

This ADR intentionally frames product and architecture boundaries. Exact table schemas, API routes, UI layout, and library choices are left to the follow-up implementation issues and the module spec. Issue #147 remains open as an epic; implementation must happen through smaller follow-up issues.

## Consequences

### Positive

- Mail integration stays aligned with RustShare's architecture instead of creating a parallel system.
- Private mail remains private by default; users must explicitly choose to share.
- Imported artifacts gain the same audit, permission, and linking benefits as other RustShare content.
- Storage direction is consistent with the move toward object-store-centered durability.
- AI/RAG boundaries are conservative, reducing the risk of leaking private correspondence.

### Negative

- Users cannot treat RustShare Mail as a full live webmail client in early versions.
- Outbound sending is unavailable until SMTP is added later.
- Import workflows require deliberate user action rather than automatic full-mailbox mirroring.
- Some users may expect Gmail/Outlook parity and need clear messaging about scope.

### Trade-offs

- Explicit import is slower than automatic sync but keeps storage, permissions, and audit costs predictable.
- Object storage for bodies/attachments adds operational consistency but requires careful handling of large mailboxes and retention policies.
- Opt-in AI indexing preserves privacy but reduces the immediacy of mail-powered retrieval features.

## Rejected Alternatives

- **Building a full standalone RustShare Mail webmail product in Phase 1.** Rejected because it would duplicate the scope of general-purpose webmail, strain the existing architecture, and blur RustShare's identity as a workspace/productivity platform.
- **Automatically importing entire mailboxes and making them workspace-visible by default.** Rejected because it would violate the principle of explicit sharing and create a high risk of exposing private or sensitive correspondence.
- **Storing mail only in PostgreSQL tables.** Rejected because it conflicts with the object-store-centered storage direction ([ADR-0001](0001-ADR.md), [ADR-0025](0025-storage-layout-and-file-identity.md)) and would concentrate blob pressure in the metadata database.
- **Sending live mailbox content to AI/RAG by default.** Rejected because private mail is especially sensitive; AI access must be opt-in, policy-controlled, and permission-enforced ([ADR-0020](0020-okf-notes-reconciliation-and-rag-safety.md)).
- **Implementing SMTP/outbound sending in Phase 1.** Rejected to keep the first version focused on safe import, archive, and reference workflows. Outbound sending may be revisited once those foundations are solid.

## Acceptance Criteria

- This ADR clearly bounds what is in scope and out of scope for the Mail module.
- Permission and privacy defaults protect private mail and require explicit sharing.
- Storage direction aligns with existing RustShare architecture ([ADR-0001](0001-ADR.md), [ADR-0021](0021-file-backed-attachments-and-portability.md), [ADR-0025](0025-storage-layout-and-file-identity.md)).
- AI/RAG boundaries are conservative and explicit.
- Cross-references to [Epic-0147](../epics/0147-mail-module.md) and [Spec-Mail-Module](../specs/mail-module.md) are included.

## Rationale

These boundaries keep RustShare safe, incremental, and aligned with its durable-company-memory direction. Mail is too sensitive to mirror blindly: it contains personal, legal, and commercial content that must respect the same permission, audit, and AI boundaries as files and notes. By treating imported mail as a first-class RustShare artifact, the module gains tenant isolation, explicit sharing, object-backed durability, and policy-driven AI access without turning RustShare into an uncontrolled mailbox or a full webmail clone. The phased approach also lets the team validate import and archive workflows before adding live sync or outbound sending.

## Migration Guidance

No migration is required for the ADR itself. When the Mail module is implemented, existing RustShare workspaces should treat mail as a new artifact type with default-private permissions. Imported mail should be stored in the tenant-scoped object storage layout defined in [ADR-0025](0025-storage-layout-and-file-identity.md), and its metadata should follow the same tenant-isolation rules as other workspace content ([ADR-0031](0031-tenant-isolation-share-links-and-rls.md)).
