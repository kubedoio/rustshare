# RustShare Mail Webclient Remediation Plan

Source audit: `docs/audits/mail-webclient-complete-audit.md`

Status legend: `todo`, `in progress`, `done`, `deferred`.

## Batch A - Security and data integrity

| Status | Findings | Files | Tests | Expected behavior |
|---|---|---|---|---|
| done | M-P1-1 Bcc lost through draft send | `backend/crates/core/src/services/email_service.rs`; `backend/server/src/services/mail_service.rs`; `backend/server/src/handlers/mail.rs`; `frontend/src/lib/components/modules/MailComposeModal.svelte` if response shape changes | Added draft EML unit test; remaining mock SMTP and DB row assertions still useful | Draft EML preserves Bcc for parsing; SMTP sends still omit Bcc headers |
| done | M-P2-5 plaintext user SMTP | `backend/server/src/handlers/mail.rs`; `backend/server/src/services/mail_service.rs`; `frontend/src/routes/(app)/settings/+page.svelte` | Added handler validation test; service rejects `tls_mode: "none"`; SMTP UI no longer offers plaintext | User mail credentials are rejected for plaintext SMTP before persistence or send |
| done | M-P2-4 remote images auto-load | `backend/server/src/handlers/mail.rs`; `frontend/src/lib/editor/adapter/security.ts`; `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte` | Added backend and frontend sanitizer tests that neutralize remote `<img>` sources | HTML mail does not load remote tracking content by default |
| done | M-P2-15 draft send validation bypass | `backend/server/src/handlers/mail.rs`; `backend/server/src/services/mail_service.rs` | Added service validation unit tests | Draft sends pass through service validation before SMTP |
| done | M-P2-16 draft attachment loss | `backend/server/src/services/mail_service.rs` | Added unit tests for draft attachment file-id extraction and missing backing file rejection | Missing attachments fail before SMTP instead of being silently dropped |
| done | M-P2-17 duplicate send prevention | `backend/server/src/handlers/mail.rs`; `backend/server/src/services/mail_service.rs`; `backend/migrations/20260716000000_mail_attachment_content_id.sql`; `frontend/src/lib/components/modules/MailComposeModal.svelte` | SMTP integration test retries one idempotency key and expects one server delivery, and verifies failed preflight does not consume the key; draft send uses a per-draft advisory lock | Retries and double-clicks do not duplicate outbound mail; locally correctable failures remain retryable |
| done | M-P2-11 blob lifecycle | `backend/migrations/20260717000000_reference_aware_blob_gc.sql`; `backend/crates/storage/src/metadata.rs`; `backend/server/src/retention.rs`; `backend/server/src/bootstrap.rs` | Corrective migration queues removed keys; retention rechecks mail/file/file-version references before deletion | Deleted mail and expired file versions are reclaimed after a 24-hour grace period without deleting shared objects |

## Batch B - Broken core workflows

| Status | Findings | Files | Tests | Expected behavior |
|---|---|---|---|---|
| done | M-P1-2 reply/forward bodies empty | `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte`; `frontend/src/lib/mail/compose.ts` | Added helper tests for quoting and HTML-to-text fallback | Replies quote original text; forwards include original body |
| done | M-P1-3 reply-all includes self/duplicates | `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte`; `backend/server/src/handlers/mail.rs` | Added frontend helper test and backend handler unit test for server-authoritative sender removal/deduplication | Reply-all excludes the authenticated SMTP From address and dedups recipients case-insensitively |
| done | M-P1-4 Move is Archive | `frontend/src/lib/components/modules/MailModuleView.svelte` | Removed misleading fake Move button; real picker test remains for future implementation | UI no longer exposes a Move action that secretly archives |
| done | M-P2-1 classic search absent | `backend/server/src/services/imap_client.rs`; `backend/server/src/handlers/mail.rs`; `backend/crates/storage/src/metadata.rs`; `frontend/src/lib/api/mail.ts`; `frontend/src/lib/components/modules/MailModuleView.svelte` | IMAP query escaping and frontend API pagination/search tests | Current mailbox and imported mail support ownership-scoped classic text search with pagination and clear-state restoration |
| done | M-P2-7 special-use folders ignored | `backend/server/src/services/imap_client.rs`; `backend/server/src/handlers/mail.rs`; `frontend/src/lib/components/modules/MailModuleView.svelte` | Modified UTF-7/special-use unit tests and explicit destination validation test | Archive/trash target advertised server folders; unresolved actions are disabled and API requests require a destination |
| done | M-P2-9 import job list unrouted | `backend/server/src/routes.rs`; `backend/server/src/handlers/mail.rs`; `backend/server/src/openapi.rs`; `frontend/src/lib/api/mail.ts`; `frontend/src/lib/components/modules/MailModuleView.svelte` | Frontend API route test; durable list is polled every three seconds | Import status survives reload and refreshes while jobs run |
| done | M-P2-14 bad empty-state CTA | `frontend/src/lib/components/modules/MailModuleView.svelte` | Covered by frontend type/lint checks; component test remains useful | Import CTA starts mail import, not Files navigation |
| done | M-P2-22 Sent append failure hidden | `backend/server/src/services/mail_service.rs`; `backend/server/src/handlers/mail.rs`; `frontend/src/lib/api/mail.ts`; `frontend/src/lib/components/modules/MailModuleView.svelte` | Response carries `append_failed`; frontend shows a warning after successful delivery | Send succeeds but clearly warns when IMAP Sent append failed |
| done | M-P2-23 forward drops mail-only attachments | `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte` | Forward warns when a visible attachment has no promoted file | Forward never silently drops visible attachments |
| done | M-P2-25 drafts mixed into imported list | `backend/crates/storage/src/metadata.rs`; `backend/server/src/services/mail_service.rs`; `frontend/src/lib/components/modules/MailModuleView.svelte` | Added service predicate test for imported-list draft exclusion | Drafts only appear in draft workflows |
| done | M-P2-12 draft artifact folders orphaned | `backend/server/src/services/mail_service.rs` | Draft replacement and send paths share artifact cleanup already covered by draft service integration paths | Updating or sending a draft removes its superseded RustShare artifact folder |

## Batch C - Reliability and performance

| Status | Findings | Files | Tests | Expected behavior |
|---|---|---|---|---|
| done | M-P1-5 mailbox mutation backend test gap | `backend/server/src/services/mail_service.rs`; `backend/server/src/services/imap_client.rs` | Added mock IMAP unit tests for mark read/unread, move, archive, trash, delete, UIDVALIDITY mismatch, no MOVE, and no UIDPLUS | Destructive IMAP operations are covered without live IMAP |
| done | M-P2-2 unbounded imported list | `backend/crates/storage/src/metadata.rs`; `backend/server/src/services/mail_service.rs`; `backend/server/src/handlers/mail.rs`; new migration | Frontend cursor contract test; SQL query is owner-scoped, bounded, stable, and draft-filtered | Imported/draft lists are bounded, ordered, SQL-filtered, and indexed |
| done | M-P2-3 phantom send id | `backend/server/src/services/mail_service.rs`; `backend/server/src/handlers/mail.rs`; `frontend/src/lib/api/mail.ts` | Response contract represents `message_id: null, stored: false` | API never returns an unpersisted message id |
| done | M-P2-8 oldest-first page ordering | `backend/server/src/services/imap_client.rs` | Existing implementation sorts UID pages newest-first; full fake-session test still useful | Mailbox pages return newest first |
| done | M-P2-18 mailbox load-more O(n^2) | `backend/server/src/services/imap_client.rs`; `backend/server/src/handlers/mail.rs`; `frontend/src/lib/api/mail.ts`; `frontend/src/lib/components/modules/MailModuleView.svelte` | Added compatible `next_cursor` response field; frontend uses cursor-based page fetches | Load more fetches one stable next page instead of refetching all prior pages |
| done | M-P2-19 duplicate fetch on switch | `frontend/src/lib/components/modules/MailModuleView.svelte` | Removed the second explicit refetch; component suite covers account/folder flow | Account/folder changes trigger one server fetch |
| done | M-P2-21 direct send does not refresh | `frontend/src/lib/components/modules/MailModuleView.svelte`; component test | Component test asserts imported-page query runs again after send | Sent/imported list state updates after send |
| done | M-P2-24 selected-import safety coverage | `backend/server/src/services/mail_service.rs`; `backend/tests/mail_imap_import_test.rs` | Added direct production-logic tests for UIDVALIDITY mismatch and complete/in-flight/abandoned row decisions; live provider test remains environment-gated | Retry safety decisions are deterministic without a public IMAP host |
| done | M-P2-26 outbound Message-ID missing | `backend/crates/core/src/services/email_service.rs`; `backend/server/src/services/mail_service.rs`; `backend/tests/mail_smtp_send_test.rs` | SMTP wire test asserts a generated Message-ID header | Direct and draft sends emit a stable standards-compliant Message-ID |
| done | M-P2-27 non-atomic draft update | `backend/server/src/services/mail_service.rs` | Replacement is fully imported before old row/folder cleanup; missing old draft returns not found | Failed draft replacement retains the previous draft |

## Batch D - UI/UX consistency

| Status | Findings | Files | Tests | Expected behavior |
|---|---|---|---|---|
| done | M-P2-6 raw modified UTF-7 folder names | `backend/server/src/services/imap_client.rs`; `frontend/src/lib/components/modules/MailModuleView.svelte` | UTF-7 decode unit test and raw/display split | Non-ASCII folders display correctly while raw selectable name still works |
| done | M-P2-10 raw UUID link UI | `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte` | Existing file API supplies selectable names; raw UUID inputs removed | Linking uses a discoverable file picker and displays file names |
| done | M-P2-13 broken inline CID images | parser/domain/storage/handler paths plus corrective migration | MIME parser test and authenticated CID rewrite sanitizer test | Inline images render through authenticated RustShare file preview URLs |
| done | M-P2-20 reader guard missing | `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte` | Frontend type/component validation | Deep mail route shows module-disabled UX consistently |
| todo | Loading, empty, error, confirmation, responsive, accessibility basics | Mail module/settings components touched above | Focused frontend tests for changed states | Core mail actions have clear states, confirmations for destructive actions, and usable responsive layout |

## Batch E - Refactor and cleanup

| Status | Findings | Files | Tests | Expected behavior |
|---|---|---|---|---|
| done | R1 extract mockable IMAP ops boundary | `backend/server/src/services/mail_service.rs`; `backend/server/src/services/imap_client.rs` | Same as M-P1-5 | One shared select/UIDVALIDITY/capability guard path for mailbox mutations |
| done | R2 list query boundary | `backend/crates/storage/src/metadata.rs`; migration; SQLx metadata | Same as M-P2-2/M-P2-25 | Mail list queries are paginated and source-mode aware |
| todo | R3 decide dead `/api/v1/mail/send` admin relay | `backend/server/src/handlers/mail.rs`; `backend/server/src/routes.rs`; `frontend/src/lib/api/mail.ts`; OpenAPI | Removal compile/type tests or route tests if retained | No unowned mail send path remains undocumented |
| todo | R4 remove dead settings folder-mapping UI | `frontend/src/routes/(app)/settings/+page.svelte` | Settings render test | UI does not expose controls with no backend behavior |
| todo | Cleanup after fixes | OpenAPI, SQLx metadata, docs, `CHANGELOG.md` if user-visible | Repo checks listed below | No stale TODOs/debug logs/duplicate types/obsolete routes from remediated areas |

## Required verification

- Backend: inspect scripts first; run `cargo fmt --check`; `SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings`; `SQLX_OFFLINE=true cargo test --all-features --lib`; `SQLX_OFFLINE=true cargo check --workspace`; mail integration tests; `cargo sqlx prepare --workspace --check` if database/queries change.
- Frontend: inspect package scripts first; run `npm run check`; `npm run lint`; `npm run test`; `npm run build`.
- Manual or documented acceptance flow: account setup, IMAP/SMTP tests, folder browse, pagination, open plain/html, attachment download, read/unread, move/archive/trash/delete, compose/reply/reply-all/forward, draft refresh/reopen/send, search, import, open artifact, cross-user denial, no secrets in UI/API/logs/audit.
