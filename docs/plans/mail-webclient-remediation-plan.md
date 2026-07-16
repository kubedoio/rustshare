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
| todo | M-P2-17 duplicate send prevention | `backend/server/src/handlers/mail.rs`; `backend/server/src/services/mail_service.rs`; possible new migration for idempotency/status | Concurrent draft send sends exactly once; repeated direct send idempotency key sends once | Retries and double-clicks do not duplicate outbound mail |
| todo | M-P2-11 blob lifecycle | `backend/server/src/services/mail_service.rs`; `backend/crates/storage/src/metadata.rs`; object store layer | Retention/discard lifecycle test documents or removes unreferenced blobs | Mail deletion/retention does not leave privacy-sensitive objects unintentionally reachable |

## Batch B - Broken core workflows

| Status | Findings | Files | Tests | Expected behavior |
|---|---|---|---|---|
| done | M-P1-2 reply/forward bodies empty | `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte`; `frontend/src/lib/mail/compose.ts` | Added helper tests for quoting and HTML-to-text fallback | Replies quote original text; forwards include original body |
| done | M-P1-3 reply-all includes self/duplicates | `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte`; `backend/server/src/handlers/mail.rs` | Added frontend helper test and backend handler unit test for server-authoritative sender removal/deduplication | Reply-all excludes the authenticated SMTP From address and dedups recipients case-insensitively |
| done | M-P1-4 Move is Archive | `frontend/src/lib/components/modules/MailModuleView.svelte` | Removed misleading fake Move button; real picker test remains for future implementation | UI no longer exposes a Move action that secretly archives |
| todo | M-P2-1 classic search absent | `backend/server/src/services/imap_client.rs`; `backend/server/src/handlers/mail.rs`; `backend/crates/storage/src/metadata.rs`; `backend/server/src/routes.rs`; `frontend/src/lib/api/mail.ts`; `frontend/src/lib/components/modules/MailModuleView.svelte` | IMAP criteria tests; DB ownership-scoped search tests; frontend search clear restores folder state | Current mailbox and imported mail support classic search and filters with pagination |
| todo | M-P2-7 special-use folders ignored | `backend/server/src/services/imap_client.rs`; `backend/crates/core/src/domain/mail_account.rs`; `frontend/src/lib/components/modules/MailModuleView.svelte` | LIST fixture with `\Archive`, `\Trash`, `\Sent`, `\Drafts`; disabled action test when unresolved | Archive/trash/sent/drafts target real server folders or show an actionable disabled state |
| todo | M-P2-9 import job list unrouted | `backend/server/src/routes.rs`; `backend/server/src/handlers/mail.rs`; `frontend/src/lib/api/mail.ts`; `frontend/src/lib/components/modules/MailModuleView.svelte` | Route test for per-user job list; component reload shows failed job | Import status survives reload and refreshes while jobs run |
| done | M-P2-14 bad empty-state CTA | `frontend/src/lib/components/modules/MailModuleView.svelte` | Covered by frontend type/lint checks; component test remains useful | Import CTA starts mail import, not Files navigation |
| todo | M-P2-22 Sent append failure hidden | `backend/server/src/services/mail_service.rs`; `backend/server/src/handlers/mail.rs`; `frontend/src/lib/api/mail.ts`; `frontend/src/lib/components/modules/MailComposeModal.svelte` or caller | Append-failure response test; frontend warning toast test | Send succeeds but clearly warns when IMAP Sent append failed |
| todo | M-P2-23 forward drops mail-only attachments | `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte`; possible backend promotion endpoint | Component test shows warning or materializes attachment | Forward never silently drops visible attachments |
| done | M-P2-25 drafts mixed into imported list | `backend/crates/storage/src/metadata.rs`; `backend/server/src/services/mail_service.rs`; `frontend/src/lib/components/modules/MailModuleView.svelte` | Added service predicate test for imported-list draft exclusion | Drafts only appear in draft workflows |

## Batch C - Reliability and performance

| Status | Findings | Files | Tests | Expected behavior |
|---|---|---|---|---|
| done | M-P1-5 mailbox mutation backend test gap | `backend/server/src/services/mail_service.rs`; `backend/server/src/services/imap_client.rs` | Added mock IMAP unit tests for mark read/unread, move, archive, trash, delete, UIDVALIDITY mismatch, no MOVE, and no UIDPLUS | Destructive IMAP operations are covered without live IMAP |
| todo | M-P2-2 unbounded imported list | `backend/crates/storage/src/metadata.rs`; `backend/server/src/services/mail_service.rs`; new migration | Multi-page row test; draft endpoint SQL filter test; SQLx metadata update | Imported/draft lists are bounded, ordered, SQL-filtered, and indexed |
| todo | M-P2-3 phantom send id | `backend/server/src/services/mail_service.rs`; `backend/server/src/handlers/mail.rs`; `frontend/src/lib/api/mail.ts` | Forced import failure response test | API never returns an unpersisted message id |
| done | M-P2-8 oldest-first page ordering | `backend/server/src/services/imap_client.rs` | Existing implementation sorts UID pages newest-first; full fake-session test still useful | Mailbox pages return newest first |
| done | M-P2-18 mailbox load-more O(n^2) | `backend/server/src/services/imap_client.rs`; `backend/server/src/handlers/mail.rs`; `frontend/src/lib/api/mail.ts`; `frontend/src/lib/components/modules/MailModuleView.svelte` | Added compatible `next_cursor` response field; frontend uses cursor-based page fetches | Load more fetches one stable next page instead of refetching all prior pages |
| todo | M-P2-19 duplicate fetch on switch | `frontend/src/lib/components/modules/MailModuleView.svelte` | Component test asserts one `listAccountMessages` per switch | Account/folder changes trigger one server fetch |
| todo | M-P2-21 direct send does not refresh | `frontend/src/lib/components/modules/MailModuleView.svelte` | Component/API test asserts imported list refetches after send | Sent/imported list state updates after send |
| todo | M-P2-24 decorative selected-import test | `backend/tests/mail_imap_import_test.rs`; `backend/server/src/services/mail_service.rs`; `backend/server/src/services/imap_client.rs` | Mock selected-import tests for UIDVALIDITY mismatch, dedup skip, partial-row reclaim | Import job tests run deterministically in CI |
| todo | M-P2-27 non-atomic draft update | `backend/server/src/services/mail_service.rs`; `backend/crates/storage/src/metadata.rs` | Failed update keeps old draft; concurrent update maps conflict safely | Draft updates do not lose previous content on failure |

## Batch D - UI/UX consistency

| Status | Findings | Files | Tests | Expected behavior |
|---|---|---|---|---|
| todo | M-P2-6 raw modified UTF-7 folder names | `backend/server/src/services/imap_client.rs`; `frontend/src/lib/components/modules/MailModuleView.svelte` if display/raw names split | UTF-7 decode unit test; LIST fixture | Non-ASCII folders display correctly while raw selectable name still works |
| todo | M-P2-10 raw UUID link UI | `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte`; possible resolve/search API | Component test for picker/create/list/remove link | Linking uses discoverable targets and displays names/navigation |
| todo | M-P2-13 broken inline CID images | `backend/server/src/handlers/mail.rs`; `backend/crates/core/src/domain/mail_message.rs`; parser/storage paths; reader page | Multipart/related fixture test; sanitizer preserves authenticated CID rewrite | Inline images render through authenticated RustShare URLs |
| todo | M-P2-20 reader guard missing | `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte` or shared layout | Disabled-module render test | Deep mail route shows module-disabled UX consistently |
| todo | Loading, empty, error, confirmation, responsive, accessibility basics | Mail module/settings components touched above | Focused frontend tests for changed states | Core mail actions have clear states, confirmations for destructive actions, and usable responsive layout |

## Batch E - Refactor and cleanup

| Status | Findings | Files | Tests | Expected behavior |
|---|---|---|---|---|
| done | R1 extract mockable IMAP ops boundary | `backend/server/src/services/mail_service.rs`; `backend/server/src/services/imap_client.rs` | Same as M-P1-5 | One shared select/UIDVALIDITY/capability guard path for mailbox mutations |
| todo | R2 list query boundary | `backend/crates/storage/src/metadata.rs`; migration; SQLx metadata | Same as M-P2-2/M-P2-25 | Mail list queries are paginated and source-mode aware |
| todo | R3 decide dead `/api/v1/mail/send` admin relay | `backend/server/src/handlers/mail.rs`; `backend/server/src/routes.rs`; `frontend/src/lib/api/mail.ts`; OpenAPI | Removal compile/type tests or route tests if retained | No unowned mail send path remains undocumented |
| todo | R4 remove dead settings folder-mapping UI | `frontend/src/routes/(app)/settings/+page.svelte` | Settings render test | UI does not expose controls with no backend behavior |
| todo | Cleanup after fixes | OpenAPI, SQLx metadata, docs, `CHANGELOG.md` if user-visible | Repo checks listed below | No stale TODOs/debug logs/duplicate types/obsolete routes from remediated areas |

## Required verification

- Backend: inspect scripts first; run `cargo fmt --check`; `SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings`; `SQLX_OFFLINE=true cargo test --all-features --lib`; `SQLX_OFFLINE=true cargo check --workspace`; mail integration tests; `cargo sqlx prepare --workspace --check` if database/queries change.
- Frontend: inspect package scripts first; run `npm run check`; `npm run lint`; `npm run test`; `npm run build`.
- Manual or documented acceptance flow: account setup, IMAP/SMTP tests, folder browse, pagination, open plain/html, attachment download, read/unread, move/archive/trash/delete, compose/reply/reply-all/forward, draft refresh/reopen/send, search, import, open artifact, cross-user denial, no secrets in UI/API/logs/audit.
