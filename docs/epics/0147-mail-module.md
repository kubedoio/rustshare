# RustShare Mail Module Epic

## Status

Proposed epic for issue #147.

This document defines the product and technical boundaries for the RustShare Mail module. It does not mean the full module is implemented yet.

Issue #147 remains open as an epic. Implementation must happen through smaller follow-up issues, not as a single large change.

## Problem

Many important work decisions live in email. Users currently:

- Manually copy/paste email content into notes, tasks, and archives.
- Lose the connection between mail content and project memory.
- Use existing webmail clients that are not designed as company memory systems.
- Need a safe way to preserve and reference mail without becoming an uncontrolled mailbox mirror.

RustShare needs a module that lets teams import, archive, organize, and reference email inside workspaces, so email can become part of durable team memory rather than an isolated inbox.

## Goal

RustShare Mail is a module for connecting, importing, archiving, organizing, and referencing email inside RustShare workspaces. The first version focuses on importing and referencing mail, not replacing a full mail client.

Specific goals:

- Mail import into RustShare-controlled storage.
- IMAP connection support.
- Safe archive and backup workflows.
- Ability to turn selected email content into notes, meeting notes, Kanban cards, file/document references, and future RAG memory objects.
- Permission-aware access.
- Auditability.
- User-owned or workspace-owned mail archives.
- Clear distinction between imported mail and live mailbox state.

## Non-Goals

First versions must NOT deliver:

- a complete Roundcube clone
- a full Gmail/Outlook replacement
- SMTP server implementation
- mail hosting service
- spam filtering engine
- calendar server
- contact server
- full Exchange replacement
- automatic AI processing of all emails by default
- importing all mailboxes without user consent
- making private emails visible to teams by default
- sending arbitrary automated emails from AI workflows

## Target Users

- technical SMB teams
- platform teams
- MSPs
- internal IT departments
- founders/operators who use email as operational memory
- teams that need archived project communication
- teams that want self-hosted control over important communication records

## Primary Use Cases

1. **Import selected email into RustShare** — A user selects one or more messages from an IMAP account or uploads `.eml` files. RustShare stores the mail as a permissioned artifact with metadata, body, attachments, and audit trail.

2. **Archive old mail into RustShare** — An admin or user configures an archive job for selected folders or date ranges. Mail is moved or copied into RustShare object storage with retention and access policies.

3. **Link email to a note** — An imported mail artifact is attached as a source reference or linked to a RustShare note without exposing private content to readers who lack permission.

4. **Create Kanban card from email** — A user converts an imported email into a Kanban card. The card preserves a link to the source mail artifact. The mail content is only visible to users who have permission on the mail artifact.

5. **Create meeting note context from mail** — A user attaches a mail thread or selected messages to a meeting note to provide context. The meeting note may show metadata and a link; full body exposure requires explicit sharing.

6. **Search imported mail** — Users search mail they own or have been explicitly granted access to, with filters for date, sender, folder source, and linked objects.

7. **Future permission-aware AI retrieval** — Imported mail may be included in RAG context only when an explicit indexing policy allows it and only after retrieval enforces the user's permissions on each mail artifact.

## Product Boundaries

RustShare Mail operates in two modes:

- **Live mailbox connection** — RustShare connects to a user's IMAP server and lists folders/messages. This mode is limited and sensitive. It is intended for selecting messages to import, not for day-to-day webmail.
- **Imported RustShare mail artifact** — Selected mail is imported into RustShare storage with metadata, permissions, audit trail, and optional links. This is the primary product surface.

The first version prefers selected import and archive workflows. RustShare Mail is not a full live webmail replacement.

## Architecture Overview

RustShare Mail adds the following components to the existing RustShare architecture, which uses PostgreSQL for metadata during the Public Preview migration and S3-compatible/RustFS object storage for durable blobs:

- **Mail Connector** — Pluggable adapter that normalizes access to mail sources (IMAP, `.eml` upload, future inbound address).
- **IMAP Client** — TLS-capable IMAP client for listing folders, fetching messages, and streaming selected imports. Secrets are stored encrypted.
- **Mail Import Service** — Orchestrates parsing, storage, permission assignment, and audit logging for imported mail.
- **Mail Artifact Store** — Persists imported mail bodies, RFC 822/MIME source, and attachment blobs in object storage.
- **Mail Metadata Store** — PostgreSQL tables for mail accounts, messages, parts, attachments, links, and import jobs.
- **Mail Web UI** — SvelteKit views for dashboard, accounts, imports, message detail, attachments, links, sharing, and job status.
- **Mail Permission Resolver** — Reuses RustShare workspace/tenant permission primitives to decide who can see, link, share, or delete a mail artifact.
- **Mail Linking Service** — Creates references between mail artifacts and notes, meeting notes, Kanban cards, and files without auto-exposing private content.
- **Mail Indexing Adapter** — Feeds imported mail into RustShare search/RAG infrastructure with ACL metadata, governed by an indexing policy.
- **Mail Audit Events** — Emits events for account connection, import, sharing, linking, export, and deletion.

See `docs/adr/0031-tenant-isolation-share-links-and-rls.md` for tenant isolation patterns and `docs/adr/0025-storage-layout-and-file-identity.md` for object storage conventions.

## Storage Model

### Core entities

- **mail_accounts** — Connection configuration for a mail source: IMAP server, username, encrypted credential, OAuth/app-password flag, workspace/user ownership, connection limits, and last sync status. No plaintext passwords.
- **mail_messages** — One row per imported message: Message-ID, normalized From/To/Cc/Bcc headers (with Bcc storage rules), Subject, Date, In-Reply-To, References, source folder, import timestamp, imported_by, owner/workspace, content hash, source account reference, and pointer to object storage for the raw source.
- **mail_message_parts** — MIME parts parsed from a message: content type, charset, transfer encoding, body text/HTML pointers, size, and ordering.
- **mail_attachments** — Attachment metadata and pointers to RustShare file/blob artifacts. Attachments become first-class RustShare files with their own permission inheritance rules.
- **mail_links** — References between a mail artifact and RustShare objects such as notes, meeting notes, Kanban cards, and files. Links do not override mail permissions.
- **mail_import_jobs** — Long-running import/archive jobs: account, folder/date-range selection, status, progress cursor, failure/retry state, started/completed timestamps, and owner.

### Mail artifact format

Imported email should preserve:

- original RFC 822 / MIME source when possible
- normalized metadata: Message-ID, From, To, Cc, Bcc handling rules, Subject, Date, In-Reply-To, References, mailbox/folder source, import timestamp, imported by, workspace/owner
- text body
- HTML body, sanitized before display
- attachments as RustShare file/blob artifacts
- checksum / content hash
- source account reference, if allowed

### Bcc rule

If Bcc is present in imported source, default behavior is to hide Bcc from display and from link recipients unless the importing user is the sender or an explicit workspace policy overrides. Bcc values may be stored for provenance but must be redacted in UI, API, search, and RAG context unless the viewer has the specific right to see them.

## Permission Model

### Import modes

- **Private mail import** — Default. The imported mail artifact is owned by the importing user. Only the owner, workspace admins with explicit archive visibility rights, or users explicitly granted access can view it.
- **Workspace mail import** — Imported into a workspace scope. Access is governed by workspace role and explicit object-level sharing.
- **Shared project mail import** — Imported into a specific project, folder, or shared space. Members of that space may see metadata; body access still requires explicit grant or role permission.
- **Admin archive import** — Admin-configured archive job. Imported artifacts may be owned by the workspace or a service principal. Admin visibility is logged and scoped by archive policy.

### Permission rules

- Default to private to the importing user.
- Owner can read, share, link, and delete.
- Workspace membership alone does not grant access to private mail artifacts.
- Linked object permissions do not automatically grant mail body access. Linking an email to a Kanban card or note must NOT expose private email content unless the user explicitly shares/imports it into that scope.
- Shared-with users/groups receive the minimum of granted permission and their workspace role.
- Admin visibility boundaries follow RustShare's existing admin/audit role model.
- Attachments inherit safe permissions from the imported mail artifact but can be shared independently when the user explicitly does so.

See `docs/specs/security-and-permissions.md` and `docs/security-model.md` for RustShare permission primitives.

## Security and Privacy Requirements

- No passwords stored in plaintext.
- Mail account secrets encrypted using RustShare secret encryption.
- OAuth/OIDC or app-password support where applicable; plain password authentication discouraged for production.
- Audit events for: connecting account, importing mail, sharing mail, linking mail, exporting mail, deleting mail.
- HTML sanitization before display to prevent XSS and tracking pixels.
- Attachment scanning hook or placeholder for future malware scanning.
- Size limits per message, per attachment, and per import job.
- Rate limits on IMAP connections and imports.
- Clear consent for imports; no silent full-mailbox mirroring.
- No automatic full-mailbox AI indexing by default.
- Redaction handling for sensitive content such as Bcc and quoted restricted content.
- Data deletion/export requirements: users can delete imported mail artifacts and export their own mail in a portable format; admin archive deletion follows workspace retention policy.

See `docs/agent-guides/safety-boundaries.md` for RustShare safety boundaries.

## Mail Source Modes

1. **Manual `.eml` upload** — Simplest first step. Users upload `.eml` files. RustShare parses, stores, and creates a mail artifact. Outlook `.msg` support may be considered later as a separate importer.
2. **IMAP selected import** — User connects an IMAP account, lists folders and messages, selects individual messages or threads, and imports them into RustShare.
3. **IMAP archive job** — Admin or user configures an archive job for selected folders and date ranges. The job runs incrementally and records progress and failures.
4. **(Optional future) Mail-to-RustShare inbound address** — A dedicated inbound address could create mail artifacts directly. Do NOT implement in first phase.

## User Experience

Initial UI views:

- Mail dashboard
- Connected accounts
- Import mail
- Imported messages
- Message detail view
- Attachments
- Links to Notes/Meetings/Kanban/Files
- Permission/share panel
- Import job status

The UI should be stable, clear, and familiar like traditional webmail, but RustShare Mail is not intended to clone Roundcube in the first phase. Roundcube may be referenced only as a UI clarity inspiration.

## Integration with Notes, Meetings, Kanban, and Files

### Notes

- Link an imported mail artifact to a note as a source reference.
- Convert selected mail text into note content.
- The note stores a link and visible metadata; mail body remains restricted to users with mail permission.

### Meeting Notes

- Attach a mail thread to a meeting note to provide context.
- Generate meeting context from selected mail only after explicit user action.
- Meeting attendees without mail permission see only metadata and a reference.

### Kanban

- Create a card from an imported email.
- The card preserves a source email link.
- Card members do not automatically gain access to the mail body; explicit sharing is required.

### Files

- Attachments become RustShare file artifacts.
- They inherit safe permissions from the imported mail artifact.
- Users can share attachments independently when they explicitly choose to do so, following the file identity and attachment rules in `docs/adr/0021-file-backed-attachments-and-portability.md` and `docs/adr/0025-storage-layout-and-file-identity.md`.

## AI / RAG Boundaries

RustShare Mail takes a conservative approach to AI/RAG:

- Imported mail is not automatically indexed for AI unless a policy explicitly allows it.
- Live mailbox content is never sent to AI by default.
- RAG retrieval must enforce permissions before context is sent to any model.
- Answers must cite source mail artifacts.
- Users/admins configure an indexing policy: disabled, metadata only, selected messages only, workspace-approved mail only.
- Deletion or permission revocation must remove mail from active retrieval.

These boundaries align with the permission-aware RAG rules in `docs/adr/0020-okf-notes-reconciliation-and-rag-safety.md`.

## Deployment Considerations

- IMAP connectivity from the backend container requires outbound network access.
- TLS verification must be configurable but strict by default.
- App passwords and OAuth secrets must be stored and rotated using RustShare secret encryption.
- Object storage bucket requirements follow existing RustShare blob storage conventions.
- PostgreSQL migrations are needed for mail metadata tables during the Public Preview phase.
- Background workers run import jobs asynchronously.
- Backup/restore must include imported mail artifacts and their metadata.
- Secret rotation must not invalidate already imported mail artifacts.
- Rate limits protect both RustShare and upstream mail servers.
- Observability and logging must not leak mail content, message bodies, credentials, or PII. Log metadata, job status, and errors only.

## Implementation Phases

### Phase 0 — Specification and Architecture

Deliver this epic, any needed ADRs, and interface mocks. No product code.

### Phase 1 — Mail Artifact Foundation

- Data model for `mail_messages`, `mail_message_parts`, `mail_attachments`.
- Mail artifact type and identity rules.
- `.eml` upload/import.
- Metadata extraction and normalization.
- Object storage persistence for raw source and bodies.
- Basic detail view.
- Tests.

Outbound sending is not in this phase.

### Phase 2 — Linking to RustShare Objects

- Link imported mail to notes.
- Link imported mail to Kanban cards.
- Link imported mail to meeting notes.
- Attachments as RustShare file artifacts.
- Audit events for linking and sharing.

### Phase 3 — IMAP Selected Import

- Encrypted IMAP account configuration.
- Account connection and folder/message listing.
- Selected message import.
- Import job status and progress.

### Phase 4 — Archive Jobs

- Folder/date-range archive jobs.
- Incremental sync/archive state.
- Retention policy support.
- Failure/retry handling.

### Phase 5 — Search and Permission-Aware Indexing

- Full-text search over imported mail.
- Optional indexing policies.
- Permission-aware retrieval hooks.
- Source citations in answers.

### Phase 6 — Webmail-Like Enhancements

- Threaded view.
- Reply/forward draft integration.
- Richer mailbox browsing.
- Optional outbound email support (SMTP send), if ever implemented.

## Follow-Up Issues

After the epic is merged, open the following GitHub issues:

### mail: define database schema for imported mail artifacts

- Description: Create PostgreSQL migrations and Rust types for `mail_messages`, `mail_message_parts`, `mail_attachments`, `mail_links`, and `mail_import_jobs`.
- Acceptance criteria: Migrations apply cleanly; types compile; relationships and indexes documented.
- Suggested difficulty: medium
- Dependencies: Phase 0 completion

### mail: implement .eml upload and import

- Description: Allow users to upload `.eml` files and create mail artifacts with normalized metadata, sanitized bodies, and attachments.
- Acceptance criteria: Upload endpoint exists; parser extracts required metadata; artifacts persist in object storage; tests cover parsing edge cases.
- Suggested difficulty: medium
- Dependencies: database schema

### mail: add mail artifact detail view

- Description: Add a SvelteKit view that shows imported mail metadata, sanitized HTML/text body, attachments, and source account reference.
- Acceptance criteria: Detail view loads; HTML is sanitized; attachments list correctly; unauthorized users are rejected.
- Suggested difficulty: medium
- Dependencies: `.eml` upload and import

### mail: link imported mail to notes

- Description: Support linking a mail artifact to a RustShare note as a source reference.
- Acceptance criteria: Link is created; note displays metadata; mail body is not exposed to note readers without mail permission; audit event emitted.
- Suggested difficulty: medium
- Dependencies: mail artifact detail view, note linking API

### mail: link imported mail to Kanban cards

- Description: Support creating or linking a Kanban card from a mail artifact while preserving source link and permission boundary.
- Acceptance criteria: Card can be created from mail; source link preserved; card members without mail permission see metadata only; audit event emitted.
- Suggested difficulty: medium
- Dependencies: mail artifact detail view, Kanban API

### mail: add encrypted IMAP account configuration

- Description: Add UI and backend support for configuring an IMAP account with encrypted credential storage.
- Acceptance criteria: Account can be saved; credentials are encrypted; connection test endpoint works; no plaintext storage.
- Suggested difficulty: medium
- Dependencies: database schema, secret encryption service

### mail: implement selected IMAP message import

- Description: Allow users to list IMAP folders/messages and import selected messages into RustShare.
- Acceptance criteria: Folder listing works; message listing works; selected messages import into artifact store; progress is reported.
- Suggested difficulty: large
- Dependencies: encrypted IMAP account configuration, mail artifact foundation

### mail: add mail import job status and retry handling

- Description: Persist and surface import/archive job status, progress, failures, and retries.
- Acceptance criteria: Jobs are persisted; status UI updates; failures are retryable; rate limits respected.
- Suggested difficulty: medium
- Dependencies: IMAP selected import

### mail: define mail AI indexing policy

- Description: Design and implement the indexing policy model that governs whether imported mail is included in RAG.
- Acceptance criteria: Policy can be disabled, metadata-only, selected-only, or workspace-approved; retrieval enforces permissions; citations work.
- Suggested difficulty: large
- Dependencies: permission model, search infrastructure

### mail: add permission-aware mail search

- Description: Add full-text search over imported mail with permission-aware filtering.
- Acceptance criteria: Users search only mail they can access; results include metadata and links; large result sets paginate.
- Suggested difficulty: large
- Dependencies: indexing policy, search infrastructure

## Acceptance Criteria

The documentation/spec PR is complete when:

- Issue #147 is represented as a proper epic.
- The Mail module goal is clearly defined.
- The Mail module is explicitly part of RustShare, not a standalone mail product.
- Non-goals prevent accidental full webmail scope creep.
- Storage, permissions, security, and privacy boundaries are defined.
- IMAP is described as a source/import mechanism, not the whole product.
- `.eml` upload/import is identified as the simplest first implementation phase.
- Integration points with Notes, Meetings, Kanban, Files, and future RAG are defined.
- Implementation phases are realistic and ordered.
- Follow-up issues are small enough for future agents/contributors.
- No product code is changed.
- No claim is made that RustShare Mail is already implemented.
