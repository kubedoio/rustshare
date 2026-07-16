# RustShare Mail — Complete Webmail Client Audit

- **Date:** 2026-07-16
- **Branch:** `audit/mail-webclient` (base: `main` @ `34602b9c` "feat(mail): Phase 7 classic webmail operations (#162)")
- **Type:** Analysis-only. No features implemented, no code refactored, no PR created. RAG/AI/semantic search was not started and is reported on only to confirm absence of coupling.
- **Method:** 14 parallel evidence-based area audits tracing complete user flows (frontend route → component → API client → REST route → handler → service → IMAP/SMTP/storage → DB/object storage/audit events → tests), followed by independent spot-verification of every P1/P2-top finding against the cited source lines by the lead auditor. Where two auditors disagreed (save→send race claim), the vendored dependency source was read to adjudicate.

---

## 1. Executive verdict

**Usable but requires blocker fixes.**

RustShare Mail is a real, architecturally sound webmail client: per-user IMAP/SMTP accounts with AES-256-GCM-encrypted credentials, genuine SSRF and TLS protections, folder browsing, live mailbox listing, sanitized message reading, compose/reply/reply-all/forward over per-user SMTP, server-side drafts, durable RustShare artifact import with idempotent dedup, recurring archive jobs with UIDVALIDITY handling, and permission-checked linking to RustShare objects. Ownership and tenant isolation are enforced uniformly and tested.

It is **not yet a complete webmail client**: five core-flow defects (P1) — silent Bcc loss on draft send, replies/forwards that carry no original content, reply-all that mails the sender back to themselves, a Move button that is secretly Archive, and zero automated tests for every Phase 7 mailbox mutation — must be fixed before normal user testing. Classic search is entirely absent, and a set of reliability/UX gaps (unbounded list queries, phantom send confirmations, tracking pixels, broken inline images) separates it from a polished daily driver.

| Priority | Count | Meaning |
|---|---|---|
| P0 (security / data-loss / credential exposure) | **0** | None found — see §5 for what was verified clean |
| P1 (core user flow broken) | **5** | §4.1 |
| P2 (reliability or major UX problem) | **27** | §4.2 |
| P3 (polish / cleanup) | **30** | §4.3 |
| Future (intentionally deferred) | 8 | §4.4 |

---

## 2. Architecture (audit §1)

### 2.1 Dependency map

```
Frontend UI
├─ routes/(app)/modules/[key]/ModulePageRenderer.svelte:24   ('mail-list' → MailModuleView)
├─ routes/(app)/modules/mail/messages/[messageId]/+page.svelte (reader, reply/forward, links)
├─ routes/(app)/settings/+page.svelte?tab=mail               (account/IMAP/SMTP config UI)
│        │
│        ▼  frontend/src/lib/api/mail.ts  (mailApi — complete REST client; no secret fields in types)
│        │
│        ▼  backend/server/src/routes.rs:262-416  mail_routes() → merged main.rs:106
│           AuthN: AuthenticatedUser extractor per handler
│           Module gate: require_mail_enabled() (handlers/mail.rs:23-41) on all 46 handlers
│        │
│        ▼  backend/server/src/handlers/mail.rs (2,368 LoC; validator::Validate + manual checks)
│        │
│        ▼  backend/server/src/services/mail_service.rs (4,150 LoC, ~45 methods)
│           ├─ services/imap_client.rs      ImapClient/ImapSession over async-imap; SSRF-pinned connect (126-156)
│           ├─ core/services/email_service.rs  lettre SMTP; system path load_config() (259-274);
│           │                                 user path send_user_email_via_smtp() (204-239)
│           ├─ crates/crypto/secret_encryption.rs  AES-256-GCM encrypt/decrypt of IMAP+SMTP passwords
│           ├─ MetadataStore (mail_* queries, owner-scoped) / ObjectStore (content-addressed blobs/)
│           ├─ FileService/FolderService     /Workspace/Mail artifact folders; attachment→file promotion
│           └─ EventStore + EventBroadcaster  Mail* domain events (sync event store, not admin audit log)
│
└─ mail_import_worker.rs — claims mail_import_jobs (SKIP LOCKED) → process_import_job / process_archive_job
```

Storage: 10 mail migrations (`20260707150001` … `20260712000000`) create `mail_messages`, `mail_message_parts`, `mail_attachments`, `mail_links`, `mail_accounts`, `mail_import_jobs` (+ archive columns), `mail_smtp_settings`, with a partial unique dedup index (`NULLS NOT DISTINCT`) on the source key.

### 2.2 Architectural assessment

Boundaries are clean in the important places: handlers are thin, IMAP is abstracted behind `ImapSession`, SMTP behind `EmailService`, and the system-vs-user SMTP separation is enforced in code (§5). Identified issues:

- **`mail_service.rs` is a god object** — 4,150 lines owning accounts, IMAP browsing, import jobs, archive jobs, SMTP settings, outbound send, drafts, and linking. Concrete risk: every change touches one file; only the archive path got a testable trait seam (`ImapArchiveSession`), which is *why* Phase 7 ops are untestable today (P1-5).
- **`handlers/mail.rs` (2,368 LoC)** — 25 DTOs + 40 handlers; mostly utoipa boilerplate, tolerable.
- **Frontend god components** — `MailModuleView.svelte` (1,021 LoC: accounts/folders/mailbox/drafts/imports/archive/compose) and `settings/+page.svelte` (1,851 LoC with the entire mail settings inline).
- **No circular dependencies** found; duplication is horizontal (reply/reply-all/forward handler bodies; duplicated module defaults in `registry.ts` vs `module_service.rs`).
- **Draft EML building is coupled to SMTP settings shape** — `save_draft` fabricates a fake `MailSmtpSettings { host: "localhost", … }` (`mail_service.rs:3096-3113`) just to call the EML builder.

---

## 3. Feature matrix

Status: **I**mplemented / **P**artial / **M**issing / **B**roken. Test column refers to meaningful automated coverage.

| # | Feature (product objective) | Status | Tests | Notes / finding refs |
|---|---|---|---|---|
| 1 | Per-user IMAP account configuration | I | partial | AES-256-GCM secrets; test-connection works; UI offers TLS modes the backend rejects (P3-1) |
| 2 | Per-user SMTP configuration | I | covered | isolation tested; accepts `tls_mode: none` (P2-5); test sends a real email to self |
| 3 | Account & identity management | P | partial | no disable toggle in UI, no default account, no From identities; arbitrary `from_address` (P3-2/3/4) |
| 4 | Folder browsing | P | not tested | mUTF-7 undecoded (P2-6); no special-use attrs (P2-7); flat nested list; no refresh; no persistence (P3-18) |
| 5 | Paginated mailbox browsing | P | not tested | cursor path dead, Load-more is O(n²) (P2-18, P3-19); within-page order inverted (P2-8) |
| 6 | Safe message reading | I | covered | double sanitization; **remote images load** (P2-4); inline CID broken (P2-13); reader page itself untested (P3-31) |
| 7 | Compose | I | covered | good validation; no list refresh after send (P2-21) |
| 8 | Reply | P | **not tested** | no quoted content (P1-2); ignores Reply-To (P3-7) |
| 9 | Reply all | B | **not tested** | includes self, no dedup (P1-3) |
| 10 | Forward | P | **not tested** | no original body (P1-2); silently drops mail-only attachments (P2-23) |
| 11 | Drafts | P | **not tested (backend)** | Bcc loss (P1-1); folder/content orphans (P2-12/27); validation bypass (P2-15); silent attachment loss (P2-16) |
| 12 | Sent mail | P | partial | no Message-ID (P2-26); append failure invisible (P2-22); phantom id fallback (P2-3) |
| 13 | Read/unread | I | **not tested** | UIDVALIDITY-guarded; backend untested (P1-5) |
| 14 | Move | B | **not tested** | UI destination hardcoded to archive folder (P1-4) |
| 15 | Archive | I | **not tested** | name-heuristic target; missing folder → generic 502 (P2-7) |
| 16 | Trash | I | **not tested** | same as archive |
| 17 | Safe permanent deletion | I | **not tested** | UIDPLUS-gated UID EXPUNGE, `confirm()` gate; safe by construction |
| 18 | Attachments | P | partial | inbound promotion + filename safety tested; draft/forward lifecycle gaps (P2-16/23) |
| 19 | Classic search and filters | **M** | n/a | absent at every layer (P2-1); dead `searchPlaceholder` config |
| 20 | Import into durable artifacts | I | partial | dedup/idempotency tested; job status session-ephemeral (P2-9); selected-import test is decorative (P2-24) |
| 21 | Linking mail to RustShare objects | I (backend) / P (frontend) | covered | backend permission-checked + idempotent; UI requires raw UUIDs (P2-10) |
| 22 | Remote vs durable distinction | P | partial | no imported marker in live list; drafts mixed into imported list (P2-25); "archive" naming collision (P3-30) |
| 23 | RAG/AI coupling | absent (correct) | n/a | no embeddings/RAG anywhere in mail paths; phase docs explicitly defer it |

Webmail-feel assessment (audit §3): **mostly a webmail client** — real compose/reply/drafts/folders/read-badges chrome. Engineering residue: account cards show `username · host:port`, the unified list is labeled "Imported RustShare mail" rather than "All mail", raw job counters (`retries n/m`), and the link form demands a raw target UUID. The registry description ("Import, archive, and reference email") predates Phase 7.

---

## 4. Findings by priority

### 4.0 P0 — none

No credential exposure, cross-tenant/cross-user access, or data-loss-without-recourse defect was found. The closest candidates were verified safe: Bcc never appears on the wire (lettre `drop_bcc` confirmed in vendored source), UID EXPUNGE is UIDPLUS-gated, SSRF pinning prevents DNS rebinding, and plaintext IMAP is rejected. The silent Bcc *delivery* loss is real but is a sender-side correctness bug → P1-1.

### 4.1 P1 — core user flow broken

---

**M-P1-1 — Bcc recipients are silently dropped when a draft is saved and later sent**

- **File/function:** `backend/crates/core/src/services/email_service.rs:419-421` (`build_outbound_message`), `:250-257` (`build_raw_draft_eml`); `backend/server/src/services/mail_service.rs:3291` (`send_draft`)
- **Observed:** Bcc is added to the SMTP envelope only, never as a header — correct on the wire, but drafts are serialized through the *same* builder, so the stored draft EML contains no Bcc anywhere. The draft is persisted by re-parsing that EML → `bcc_addresses = []`; `send_draft` reads recipients from the re-parsed message. Reopening the draft in the UI confirms the loss (`MailModuleView.svelte:355`). A user who saves a draft with Bcc sends mail that silently never reaches the Bcc recipients. The sent local copy of every outbound mail also records `bcc_addresses: []` (sender can never audit who was Bcc'd).
- **Expected:** draft Bcc preserved end-to-end; sent local copy retains the Bcc list (owner-visible only).
- **Correction:** persist recipient lists as first-class columns/JSONB for drafts (or add a `Bcc` header to the draft EML only, stripping it at send); store Bcc on the outbound message record, not the wire message.
- **Required tests:** draft save→load round-trip asserting `bcc_addresses` survives; `send_draft` against a mock SMTP asserting `RCPT TO` includes the Bcc address and the transmitted bytes contain no `Bcc:` header; sent artifact row retains `bcc_addresses`.

---

**M-P1-2 — Reply and forward bodies contain no original content**

- **File/function:** `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte:197-199` (`openReply`), `:212-214` (`openReplyAll`), `:225-229` (`openForward`)
- **Observed:** the reply body ends at a bare `"> "` — zero quoted lines; the forward body contains only the `---------- Forwarded message ----------` header block. Users routinely send replies/forwards with none of the original text.
- **Expected:** reply quotes the plain-text body with `> ` prefixes; forward includes the original body in the forwarded block.
- **Correction:** the text part is already fetched for display (`bodyContent`); build the quoted/forwarded body from it (strip HTML to text when only HTML exists).
- **Required tests:** component tests for `openReply`/`openForward` asserting quoted content is present.

---

**M-P1-3 — Reply-all includes the current user and does not dedup recipients; backend reply/reply-all are byte-identical**

- **File/function:** `+page.svelte:204-217` (`openReplyAll`); `backend/server/src/handlers/mail.rs:2047-2113` (`reply_mail_handler` vs `reply_all_mail_handler`)
- **Observed:** `composeTo = [from_address, ...to_addresses]` — the user's own address (present in To of mail they received) stays in the recipient list; a From that is also in To is duplicated. The two backend handlers are byte-identical calls to `send_outbound_mail(..., req.in_reply_to_msg_id, false)` — all semantics live in the untested frontend prefill; nothing server-side would catch a regression.
- **Expected:** sending identity excluded, recipients deduped case-insensitively across To/Cc, Bcc never copied (already correct: `composeBcc = ''`).
- **Correction:** exclude the account's SMTP `from_address` and dedup in `openReplyAll`; preferably reconstruct reply-all recipients server-side where the identity is authoritative; make `/reply-all` enforce it (or collapse the duplicate routes).
- **Required tests:** prefill test with self in To/Cc and duplicate From; backend test distinguishing reply vs reply-all payloads.

---

**M-P1-4 — The "Move" button is a duplicate of Archive**

- **File/function:** `frontend/src/lib/components/modules/MailModuleView.svelte:796-807`
- **Observed:** `mailApi.moveMessage(..., archiveFolder(), ...)` — destination is hardcoded to the archive folder; identical effect to the Archive button (`:735-741`). No folder picker exists despite the API supporting arbitrary `destination_folder`. Phase 7's advertised "move" operation does not exist in the UI.
- **Expected:** move to a user-chosen folder.
- **Correction:** destination `<select>` populated from `foldersQuery`, or drop the button until it exists.
- **Required tests:** component test asserting the chosen destination is passed to `moveMessage`.

---

**M-P1-5 — All Phase 7 mailbox mutations (mark read/unread, move, archive, trash, delete) plus reply/reply-all/forward and drafts shipped with zero backend tests**

- **File/function:** `backend/server/src/services/mail_service.rs:1134-1258` (ops), `handlers/mail.rs:1112-1301, 2047-2152`; `git show --stat 34602b9c` — 210 new handler lines, only a 1-line test fix
- **Observed:** the only mockable IMAP abstraction (`ImapArchiveSession`, `imap_client.rs:535`) covers two methods used by archive jobs; every other op calls the concrete `ImapSession` via `connect_and_login` and is untestable without a live server. UIDVALIDITY-mismatch aborts, the no-MOVE refusal (`mail_service.rs:1207-1212`), and the no-UIDPLUS expunge refusal (`imap_client.rs:501-503`) are safety checks with no coverage. The one live-IMAP test requires a *public* IMAP host and passes vacuously on an empty mailbox (`mail_imap_import_test.rs:8-88`). Frontend action buttons are absent from the `MailModuleView.test.ts` mock surface. AGENTS.md requires tests for new behavior, and mail is a listed safety-boundary area.
- **Expected:** destructive and state-changing IMAP operations covered by mock-based tests.
- **Correction:** extract an `ImapOps` session trait mirroring `ImapArchiveSession` for the shared connect→login→select prelude; add unit tests per op + mismatch/refusal branches; extend the frontend mock surface and add button tests.
- **Required tests:** as above; this finding *is* the test gap.

### 4.2 P2 — reliability / major UX

Each finding: file/function — observed → expected → correction → required tests.

---

**M-P2-1 — Classic mail search is entirely unimplemented**

- `backend/server/src/services/imap_client.rs:282` (`uid_search("ALL")` is the only search); `backend/server/src/handlers/mail.rs:762-768` (`ListMailMessagesQuery { folder, limit, cursor }` only); no UI control in `MailModuleView.svelte`; no route in `routes.rs:262-415`; global `/api/v1/search` covers files/folders only (`handlers/search.rs:31-32`). `frontend/src/lib/modules/registry.ts:570-572` misleadingly declares `searchPlaceholder: 'Search messages...'`.
- Observed: no current-folder/account/imported/sent/draft search, no subject/sender/recipient/date/has-attachment/read filters at any layer. → Expected: at minimum current-folder IMAP `SEARCH` with the existing 30s timeout pattern, plus DB-backed filtered search over imported mail (`mail_messages` already has subject/from/to/sent_at/has_attachments/source_mode columns).
- Correction: add `q`/field params → IMAP SEARCH criteria in `fetch_message_summaries`; add paginated `search_mail_messages` in `metadata.rs`; search box wired to a new query key; honor or remove the registry metadata.
- Tests: criteria building + ownership scoping; IMAP timeout mapping; typing refetches and clearing restores folder state.

**M-P2-2 — Unbounded mail list queries (no LIMIT) and missing composite index**

- `backend/crates/storage/src/metadata.rs:711-735` (`list_mail_messages`: full table per render, no LIMIT, no source_mode filter); `backend/server/src/services/mail_service.rs:750-755` (`list_drafts` loads **all** owner messages then filters in Rust); only single-column `owner_id`/`tenant_id` indexes exist (`migrations/20260707150001:31-35`) for an `ORDER BY imported_at DESC` query.
- Observed: every Mail page render transfers the user's entire imported archive; drafts endpoint scans everything. → Expected: LIMIT+cursor pagination, SQL-side `WHERE account_id AND source_mode='draft'`, composite `(owner_id, imported_at DESC) WHERE deleted_at IS NULL` index.
- Tests: >1 page of rows asserts bound and stable order; drafts endpoint does not scan non-draft rows.

**M-P2-3 — Send fallback returns a phantom `message_id`**

- `backend/server/src/services/mail_service.rs:2934-2956` — when SMTP succeeds but local artifact import fails, an in-memory `MailMessage` with a fresh UUID is returned (deliberate retry-prevention), but it is never persisted: the client gets 200 + an id that 404s on open and never appears in any list; `MailMessageSent` events reference the phantom id.
- → Expected: never return an unpersisted id. Correction: persist a minimal outbound row (retry the import in background) or return a distinct `stored: false` status. Tests: forced-import-failure path asserting response semantics.

**M-P2-4 — Remote images / tracking pixels auto-load in the reader (privacy)**

- `backend/server/src/handlers/mail.rs:1491-1498` (ammonia keeps `<img src="https://…">`); `frontend/src/lib/editor/adapter/security.ts:38-62` (DOMPurify allows `img`/`src`); rendered via `{@html}` (`+page.svelte:305`); no CSP backstop (`docker/nginx.conf:157` commented out; `security_headers.rs:6` deliberately omits CSP).
- Observed: opening any imported HTML mail leaks read-receipt + IP to the sender. → Expected: remote content blocked by default with a per-message "Load remote content" opt-in, or an `img-src 'self' data:` CSP on the reader response.
- Tests: sanitizer unit test that `<img src="https://tracker/x">` is neutralized by default and restored on opt-in.

**M-P2-5 — Per-user SMTP accepts `tls_mode: "none"` → credentials in cleartext**

- `backend/server/src/handlers/mail.rs:1768` (accepts `MailTlsMode::None`); `mail_service.rs:2606-2622` (no TLS-mode validation — contrast IMAP `validate_tls_mode` at `:855-865` which rejects both `none` and `starttls`); `email_service.rs:335-343` (plaintext transport with credentials); UI offers "Plain / None" with no warning (`settings/+page.svelte:1674-1676`); admin system SMTP correctly rejects `none` (`admin/config.rs:478`).
- → Expected: reject `none` for per-user SMTP, or gate behind an explicit acknowledgement with a UI warning. Tests: PUT smtp settings `tls_mode:"none"` → 400.

**M-P2-6 — Non-ASCII IMAP folder names displayed as raw modified-UTF-7**

- `backend/server/src/services/imap_client.rs:258-264` maps `name.name()` raw; imap-proto 0.16.7 does not decode; no utf7 crate in `Cargo.lock`. Observed: `Entw&APw-rfe` shown to the user; `folderNamed` heuristics also can't match localized special folders. Round-trip selection works (raw name passed back verbatim) — display bug, not functional break. → Expected: decode mUTF-7 for display, keep raw (or re-encode) for SELECT. Correction: `imap-utf7` crate or small internal decoder; return decoded name. Tests: `&APw-` → `ü` unit test; LIST fixture with non-ASCII mailbox.

**M-P2-7 — Special-use folder attributes ignored; archive/trash can target nonexistent folders**

- `imap_client.rs:247-265` discards `Name::attributes()`; role detection is a 3-entry hardcoded name list (`MailModuleView.svelte:67-78`, incl. Gmail-only `'[gmail]/all mail'`); frontend falls back to literal `'Archive'`/`'Trash'` and backend mirrors it (`handlers/mail.rs:1222,1253`); if the folder doesn't exist the user gets a generic `502 "IMAP server error"` with no hint.
- → Expected: capture `\Sent \Drafts \Trash \Archive` (RFC 6154) into `MailFolder`, prefer attribute-based resolution, keep names as fallback; disable the action with a hint when unresolvable. Tests: LIST fixture with attributes; resolution precedence; component test with folder list lacking archive/trash.

**M-P2-8 — Within-page message order is oldest-first**

- `imap_client.rs:314-317` — UIDs are sorted newest-first for page *selection* (`:287-288`) but FETCH results are returned in server (ascending-UID) order, contradicting the `:285-286` comment and the UI caption "Showing the newest N messages" (`MailModuleView.svelte:819`). No re-sort in handler or frontend. → Expected: newest first within the page. Correction: `summaries.sort_unstable_by(|a,b| b.uid.cmp(&a.uid))` before returning. Tests: unit test over a fake fetch stream (or factor the sort into a pure function).

**M-P2-9 — Selected-import job status is session-ephemeral; the job-list backend chain is built but unrouted**

- `MailModuleView.svelte:45,179-182` (`recentImportJobs` is `$state`, lost on reload, manual per-job Refresh only); `mail_service.rs:1325` (`list_import_jobs`) + `metadata.rs:1195` (`list_mail_import_jobs_by_owner`) exist with **no route registered**; `MailImportJobListResponse` (`handlers/mail.rs:197`) is a dead DTO referenced only by openapi components. → Expected: persistent per-account job list. Correction: register `GET /api/v1/mail/import-jobs`, back the panel with it, add `refetchInterval` while any job is non-terminal. Tests: route-level list test; component test that a failed job survives reload.

**M-P2-10 — Link UI requires pasting raw UUIDs and displays raw UUIDs**

- `+page.svelte:371-384` (input "Target object ID"), `:402-405` (renders `target_type` + `target_id`, no name, no navigation). Backend linking itself is permission-checked, idempotent, and well-tested. → Expected: target picker/search and resolved display names with navigation. Correction: picker + resolve endpoint (or embed names in `MailLinkResponse`). Tests: component test for create/list/remove link.

**M-P2-11 — Mail blob objects in object storage are never deleted (storage + privacy leak)**

- `mail_service.rs:168-176, 291-298, 407-414` writes `blobs/{sha256}` for source, attachments, and body parts; **no `object_store.delete` exists in any mail path** (grep confirms only notes/avatars/admin-users delete objects); `discard_draft` deletes folder+row only; archive retention (`metadata.rs:1536-1586`) hard-deletes DB rows only. Content of "deleted"/retention-expired mail persists in object storage forever; every message is stored twice (blob + promoted file). → Expected: lifecycle-bound deletion, or a documented WORM design + refcount-aware GC (same-hash sharing with file blobs makes naive deletion unsafe). Tests: retention/discard leaves no orphan blobs.

**M-P2-12 — Draft update and draft-send orphan the draft's artifact folder**

- `mail_service.rs:3148-3156` (update hard-deletes the row) and `:3312-3315` (`send_draft`) never remove the `/Workspace/Mail/{date}-{subject}-{uuid}` folder + `source.eml` created by `import_raw_source`; only `discard_draft` does (`:3232-3237`). Every draft edit and every draft send leaves user-visible clutter containing content the user believes overwritten. → Correction: share one artifact-cleanup helper across save(update)/send/discard. Tests: update-save and send leave no orphan folder/files.

**M-P2-13 — Inline (Content-ID) images can never render**

- `handlers/mail.rs:1491-1498` strips `cid:` URLs (schemes limited to http/https/mailto); no `content_id` captured anywhere (`eml_parser.rs:25-31`, `mail_message.rs:168-182`); inline parts without filename silently dropped (`eml_parser.rs:215`). multipart/related mail renders broken images; the parts exist as ordinary attachments. → Expected: capture Content-ID, persist inline parts, expose via authenticated endpoint, rewrite `cid:` refs before sanitization. Tests: `multipart_related.eml` fixture asserting the image resolves and survives sanitization.

**M-P2-14 — Empty-state "Import mail" CTA navigates to the Files page**

- `MailModuleView.svelte:944-950`: `onAction={() => goto('/files')}`. Observed: user lands on Files; no import happens. → Expected: open the `.eml` upload picker (`uploadInput?.click()`). Tests: CTA opens the file picker.

**M-P2-15 — Send validation is bypassed via the draft path**

- `handlers/mail.rs:2154-2166` (`SaveDraftRequest` has no `Validate`), `:2271` (`send_draft_handler` never re-validates); `validate_send_outbound_mail_request` (`:1817-1854`) runs only in send/reply/forward handlers. Drafts can be sent with empty subject, >20 attachments, or zero recipients → opaque `502 "SMTP send failed"`. → Correction: run the same validator in `send_draft`; sane caps on draft save. Tests: oversized recipient list/body rejected on draft send.

**M-P2-16 — Silent attachment loss when a draft's workspace file is deleted before sending**

- `mail_service.rs:3287` (`filter_map(|a| a.file_id)` — FK is `ON DELETE SET NULL`); the mail goes out without the attachment, no warning. Related: a deleted/trashed file surfaces as misleading `Permission denied` because *any* `get_file` error maps to `PermissionDenied` (`mail_service.rs:3080-3089, 2821-2825`). → Expected: fail the send with a clear "attachment no longer available" error. Tests: draft with deleted attachment errors instead of sending incomplete mail.

**M-P2-17 — No idempotency on outbound send → duplicate emails**

- `mail_service.rs:2785+` (send happens before any durable record; no idempotency key/outbox); `send_draft` (`:3258-3318`): two concurrent requests both pass `get_draft` and both send (draft deleted only after send); a retry after an ambiguous failure (SMTP succeeded, response lost) delivers twice. UI disables the button only while pending. → Expected: client-supplied `Idempotency-Key` for direct sends; atomic draft status claim (`sending→sent`) for draft sends. Tests: concurrent `send_draft` ×2 → exactly one SMTP send; retried send with same key → one send.

**M-P2-18 — IMAP listing fetches and sorts the entire UID set on every refresh; Load-more is O(n²)**

- `imap_client.rs:282-293` (`uid_search("ALL")` + in-memory sort + `take(limit)`); frontend never uses the cursor param (`mail.ts:245-255` dead) — "Load more" grows the limit and refetches everything (`MailModuleView.svelte:427-430`), and can exceed the backend's 1000 cap → error toast after ~10 clicks; response has no `next_cursor` (`mail.rs:116-120`). → Expected: server-side UID windowing and cursor-based Load-more. Correction: `next_cursor = min(uid)` + pass cursor; page by UID ranges. Tests: cursor slicing against a fake session; component Load-more test.

**M-P2-19 — Duplicate IMAP fetches on every account/folder switch**

- `MailModuleView.svelte:149-164`: one `$effect` calls `setOptions` (triggers fetch) and a second unconditionally calls `accountMessagesQuery.refetch()`; with no AbortSignal (`client.ts:87-91`) the cancelled request still executes server-side — two full connect/login/SELECT/SEARCH cycles per switch. → Correction: drop the explicit refetch effect (keyed fetch + `staleTime: 0`), or keep refetch with `staleTime: Infinity`. Tests: `listAccountMessages` call count === 1 per switch.

**M-P2-20 — Reader deep route lacks the module-enabled guard**

- `routes/(app)/modules/mail/messages/[messageId]/+page.svelte` never consults the module registry; direct navigation with the module disabled yields a bare "Failed to load message" instead of the "Module Disabled" screen used by `/modules/mail` (`[key]/+page.svelte:34-42`). Data is safe (backend 403); UX is inconsistent. → Correction: mirror the guard in the deep route (or a shared layout guard). Tests: disabled-module render test.

**M-P2-21 — Message list not refreshed after a direct send**

- `MailModuleView.svelte:247-250` (`sendMutation.onSuccess`: close modal + toast only) vs `:279-285` (`sendDraftMutation` refetches drafts+messages). Sent mail is invisible until manual reload. → Correction: `await $importedMessagesQuery.refetch()` in `sendMutation.onSuccess`. Tests: assert `listMessages` called again after send.

**M-P2-22 — Sent-folder IMAP append failure is invisible to the user**

- `mail_service.rs:2959-3018` — a failed append correctly does *not* fail the send, but the only signal is `append_failed` inside an event payload (`:3039`); `SendMailResponse` carries only `message_id` (`mail.rs:1813-1815`); the frontend has zero handling. → Expected: `append_failed` flag in the response + a warning toast ("sent, but not saved to your Sent folder"). Tests: append-failure path asserting the flag.

**M-P2-23 — Forward silently drops "mail-only" attachments**

- `+page.svelte:233`: `attachments.map(a => a.file_id).filter(...)` — attachments never promoted to the workspace vanish from the forward with no warning. (Currently latent: import always sets `file_id`, but the reader renders a "mail-only" badge for the null case.) → Expected: materialize into the workspace on forward, or warn which attachments were dropped. Tests: forward with `file_id: null` attachment → visible warning.

**M-P2-24 — The selected-import integration test is decorative**

- `backend/tests/mail_imap_import_test.rs:8-88` — skips unless a *public* IMAP host is configured (SSRF rejects localhost, so no loopback server possible), and passes vacuously on an empty mailbox (`:83-88`). The full `process_import_job` path (UIDVALIDITY-mismatch fail, dedup skip, partial-row reclaim) has no mock-session test comparable to the archive path. → Correction: extract a mockable session trait for selected import (as done for archive), or a loopback-friendly env bypass like SMTP's; delete or fix the vacuous test. Tests: trait-level mock tests of `process_import_job`.

**M-P2-25 — Drafts are mixed into the "Imported RustShare mail" list and open the read-only reader**

- `metadata.rs:711-735` (no source_mode filter) → drafts render with a `Draft` badge in the imported list (`MailModuleView.svelte:953-991`) *and* in the Drafts panel; clicking one navigates to the read-only reader instead of compose. → Correction: exclude `source_mode='draft'` in SQL (drafts have a dedicated endpoint) or route draft clicks to `openDraft`. Tests: drafts excluded from the imported list.

**M-P2-26 — Outbound mail carries no Message-ID header**

- `backend/crates/core/src/services/email_service.rs:378-452` (`build_outbound_message` never calls `.message_id()`; lettre 0.11 only sets it on opt-in). Verified by reading the whole function. Consequence: every sent artifact has `message_id = NULL` — replies to one's own sent mail cannot thread; the Sent-folder copy lacks a Message-ID. → Correction: generate `<uuid@domain>` before send, include it, persist it on the artifact. Tests: received EML contains `Message-ID:`; artifact row has it set.

**M-P2-27 — Draft update is non-atomic delete-then-recreate; failure loses the draft**

- `mail_service.rs:3134-3156` — update hard-deletes the old row, then re-imports; an import failure in between permanently loses the previous draft content; concurrent PUT/GET in the window sees 404; concurrent same-draft saves hit a unique violation → 500. → Correction: update in place, or recreate-then-delete in one transaction; map 23505 → 409. Tests: failed update keeps the old draft; concurrent update + get consistent.

### 4.3 P3 — polish / cleanup (one line each; all evidence-backed)

**Settings/accounts**
1. IMAP TLS select offers `starttls`/`none`, backend always rejects both — guaranteed failed save; the client code paths for them are dead (`settings/+page.svelte:1420-1423,1564-1568`; `mail_service.rs:855-865`; `imap_client.rs:198-229`).
2. No UI toggle for account `is_enabled` (backend supports it; badge renders 'Disabled').
3. "Folder mapping" settings section is decorative dead UI — 5 disabled inputs, never persisted (`settings/+page.svelte:1785-1806`). *Remove before release — user-facing dead UI.*
4. "Default account" checkbox is a disabled placeholder (`settings/+page.svelte:1515-1528`).
5. Arbitrary `from_address` allowed (format-only validation) — acceptable for webmail, add a UI hint that the relay must permit it (`handlers/mail.rs:1769-1770`).

**Dead/obsolete surface** *(decide remove vs support before release)*
6. `POST /api/v1/mail/send` admin system-relay endpoint + `mailApi.sendMessage` + DTOs — no UI caller, referenced only by tests (`handlers/mail.rs:789-829`; `mail.ts:376-378`).
7. Dead backend chain: `GET /mail/accounts/{id}`, `GET`/`DELETE /mail/archive-jobs/{job_id}` (no client), `MailSourceMode::InboundAddress`, `MailVisibility` non-Private variants, `ImapSession::copy_message`, `EmailService::build_raw_eml`, frontend `MailFolderMoveRequest`, dead `is_seen` field + badge logic (`mail.ts:16`; `MailModuleView.svelte:63-65,972-976`), unused `ConfirmModal` import.

**Compose/reply details**
8. Reply ignores original `Reply-To` (never parsed; `mail_message.rs:65-103`).
9. `In-Reply-To`/`References` emitted without angle brackets (`eml_parser.rs:95-102` strips; `mail_service.rs:2863-2870` reuses bare id) — threading may break with strict clients.
10. Invalid recipient address → 502 "SMTP send failed" instead of 400 (`email_service.rs:454-458`; `handlers/mod.rs:674`).
11. Frontend `required` on To blocks Cc/Bcc-only sends the backend allows (`MailComposeModal.svelte:192-198`).
12. Message-page compose: no `draftId` tracking → every "Save draft" creates a new draft; `hasSmtp` not passed (guard bypassed); `saved = true` set optimistically even if save fails (`+page.svelte:436-451`; `MailComposeModal.svelte:140-144`).
13. Forward attachment race: reads `$attachmentsQuery.data ?? []` at click time — clicking before the query resolves forwards nothing (`+page.svelte:232-233`).
14. Saved forward drafts are sent as replies (`inReplyToMsgId` unconditional `:444`; `send_draft` hardcodes `is_forward: false`, `mail_service.rs:3308`).
15. `sendCompose` rejection propagates uncaught (console noise only — the save→send ordering itself was **verified safe**: `mutate()` rethrows, `@tanstack/query-core/src/mutation.ts:325`; the P1 race claim was adjudicated and rejected).

**Reader**
16. Every part fetch appends a `MailMessageViewed` event, and a failed event append 500s the read (`handlers/mail.rs:1560,1575,1608`) — log-and-continue + dedupe.
17. Sanitized HTML part served as `text/html` without a CSP sandbox header (`mail.rs:1561-1568`) — add `Content-Security-Policy: sandbox`.
18. No attachment indicator in the live mailbox list (`mail.rs:106-114`; needs BODYSTRUCTURE).

**Folders/list**
19. No folder-list refresh control; selection not persisted across reload; default is `folders[0]`, not INBOX (`MailModuleView.svelte:42,143,540-576`).
20. Action endpoints skip folder/destination length validation (list endpoint caps at 512) (`mail.rs:1090-1102` vs `:1060-1067`).
21. `loadSmtpSettings` last-write-wins race on rapid account switching (`MailModuleView.svelte:308-325`).
22. Stale UIDs linger in `selectedUids` after server-side expunge/move — a later import job fails for them (`MailModuleView.svelte:417-421`).

**Backend hygiene**
23. Stringly-typed status/source_mode: enum parsed in `process_archive_job` (`mail_service.rs:2416-2421`) but raw literals in `process_import_job` (`:1339`), cancellation checks (`:1472`), and the worker (`mail_import_worker.rs:77`).
24. Dual-use `in_reply_to` column: RFC Message-ID for real mail, internal draft UUID for drafts (`mail_service.rs:3174` vs `:190`) — leaks a UUID into API responses; works, undocumented.
25. `has_attachments` never persisted for drafts (`mail_service.rs:3204` mutates only the returned struct).
26. `MailMessageDraftCreated` uses `AggregateType::MailAccount` with the *message* id; updates re-emit Created (no Updated event) (`mail_service.rs:3206-3215`).
27. Draft body-part selection diverges: frontend requires `is_body` (`mail.ts:425-427`), backend takes first `text/plain` ignoring `is_body` (`mail_service.rs:3270-3275`).
28. OpenAPI param drift `{account_id}`/`{id}`/`{job_id}` (leaked into `docs/contracts/rustshare-api-openapi.json:4843`); DELETE with JSON body (`mail.rs:1264`); `_handler` suffix inconsistency; duplicate `validate_send_mail_request`/`validate_send_outbound_mail_request` bodies; error string-matching in `email_error_to_app_error` (`mail.rs:1719-1723`).
29. N+1: per-link permission queries (`mail_service.rs:713-726`); 3+ DB round trips per imported UID; `ensure_mail_root_folder` 2 queries/message (`:2553-2577`). No IMAP/SMTP connection reuse (lettre `pool` feature compiled but discarded per send). Duplicate import-job creation is safe but wasteful (no idempotency on job create).
30. Naming/semantics: "archive" means both IMAP move-to-Archive and recurring archive jobs; imported `imap_selected` mail badged "Mailbox" although it is a local copy (`MailModuleView.svelte:398-415`); no imported/remote-deleted markers anywhere, so users cannot fully distinguish the 7 lifecycle states.

### 4.4 Future — intentionally deferred (do not build now)

1. RAG / semantic search / embeddings — explicitly deferred in `docs/superpowers/specs/2026-07-11-mail-client-phase5-design.md:23`; keep decoupled from webmail.
2. Persisted folder mappings (Inbox/Sent/Archive/Drafts/Trash per account) — replace name heuristics once a real mapping API exists.
3. Multiple From identities per account with authorization.
4. IMAP folder management (create/rename/delete).
5. Remote-deletion tracking / tombstones for imported mail.
6. BODYSTRUCTURE-based attachment indicators in the live list.
7. IMAP/SMTP connection pooling / per-account session cache.
8. Refcount-aware object-storage GC for `blobs/` (needs design; naive deletion unsafe due to hash sharing).

---

## 5. Security & privacy review (audit §14) — summary

Classified security findings: **P2-4** (tracking pixels), **P2-5** (plaintext SMTP opt-in), **P2-11** (blob retention past deletion). Lower-severity: subjects + account host/username stored in event payloads (`events/types.rs:523-530,615-622` — drop subject, keep ids/counts); raw IMAP/SMTP error strings persisted to `last_error` and shown to the owner (`mail_service.rs:1080,2767` — sanitize to category + short message); mail events absent from the admin audit log surface (`handlers/admin/audit.rs:133-181` — a deliberate decision is needed, not a bug).

Verified **not vulnerable** (each traced in code):

- Cross-user/cross-tenant access — uniform `tenant_id + owner_id` enforcement on messages (`mail_service.rs:450-452`), accounts (`:956`), jobs (`:1315`), drafts (`:766`), replies (`:2859`); SQL-level owner joins on parts/attachments (`metadata.rs:469-475,504-505`); tested (`mail_read_test.rs:543`, `mail_smtp_send_test.rs:84-88`, `mail_linking_test.rs:107-112`).
- Credential encryption — AES-256-GCM, 12-byte random nonce, 32-byte key from `RUSTSHARE_SECRET_ENCRYPTION_KEY` validated at startup (`secret_encryption.rs:23-33,63-76`); real crypto, not obfuscation.
- Secret exposure — no password fields in any response DTO; `#[serde(skip_serializing)]` on domain; admin masks `"***"`; frontend test asserts the contract.
- SSRF — `resolve_public_socket_addrs` rejects localhost/private/link-local/CGNAT/multicast/IPv4-mapped (`validation.rs:27-80`); IMAP pins resolved addresses against DNS rebinding (`imap_client.rs:126-152`); SMTP same (`email_service.rs:355-369`); test escape hatch is `cfg!(debug_assertions)`-only.
- TLS — rustls + webpki roots; lettre `Tls::Wrapper`/`Required`; zero `accept_invalid` flags in repo.
- Bcc on the wire — envelope-only per-user path + lettre `drop_bcc` on system relay (vendored source verified); unit test `email_service.rs:522-548`.
- HTML injection — double sanitization (ammonia server-side, strict scheme allowlist; DOMPurify client-side forbidding `on*`, `style`, `script`, `iframe`); tested both ends.
- SMTP misuse — system relay admin-only (`handlers/mail.rs:807-811`); all user sends flow through per-user SMTP settings. **No path exists where user mail uses system SMTP.**
- Attachment permission — send/draft/forward re-check View permission + tenant per file (`mail_service.rs:2820-2829,3080-3089`).
- Destructive IMAP ops — UID-scoped expunge only, UIDPLUS-gated refusal; MOVE-capability refusal (no copy+delete fallback); per-op UIDVALIDITY guards.
- Log privacy — no subjects/bodies/credentials in any `tracing::` call in mail paths (grepped; ids/counts/hosts only).
- Header injection on download — server-generated filenames; `HeaderValue::from_str` fallback (`mail.rs:1580-1582`).
- Link-target existence leak — unreadable targets silently omitted from listings (`mail_service.rs:711-726`).

## 6. Reliability & performance (audit §15) — summary

Findings P2-2/8/17/18/19 and P3-29 above. Verified solid: 30s timeouts on every IMAP op including login/TLS/DNS; upload streaming with 25 MB cap; IMAP size pre-check before body transfer; job claiming via `FOR UPDATE SKIP LOCKED`; heartbeat-based stale-job recovery; `catch_unwind` panic isolation and 30s-drain worker shutdown; cancellation checks inside both job loops; archive exponential backoff (`POWER(2, retry_count)`); import dedup via partial unique index + `ON CONFLICT` + running-job guard. SMTP has no explicit timeout (lettre's internal 60s default) and no HTTP `TimeoutLayer` exists — acceptable today, note for hardening.

---

## 7. Refactor plan (audit §16)

### 7.1 Must refactor before release

| # | Problem | Concrete risk | Proposed boundary | Files | 
|---|---|---|---|---|
| R1 | Phase 7 ops call concrete `ImapSession` via a triplicated connect→login→select→UIDVALIDITY prelude | P1-5: destructive ops untestable | Extract `ImapOps` session trait mirroring `ImapArchiveSession`; one shared prelude | `mail_service.rs:1144-1251`, `imap_client.rs` |
| R2 | `list_mail_messages` unfiltered/unbounded; drafts filtered in Rust | P2-2/25: perf + drafts in wrong list | SQL `source_mode` filter + LIMIT/cursor + composite index migration | `metadata.rs:711-735`, `mail_service.rs:743-756`, new migration |
| R3 | `/api/v1/mail/send` admin relay + `mailApi.sendMessage` — dead but live | P3-6: sends mail with no persisted copy/audit | Decide: remove route+DTOs+frontend method, or document as admin API with tests | `handlers/mail.rs:789-829`, `routes.rs:304-305`, `mail.ts:376-378` |
| R4 | Dead settings UI shipped (folder-mapping placeholder) | P3-3: user-facing dead controls | Remove section until a mapping API exists | `settings/+page.svelte:1784-1807` |

### 7.2 Safe follow-up refactors

- Split `mail_service.rs` (4,150 LoC) by concern: accounts/browse, import/archive jobs, drafts, outbound send. Split `handlers/mail.rs` similarly. Natural, mechanical, test-covered after P1-5.
- Split `MailModuleView.svelte` into Mailbox / Jobs / Imported panels; extract the mail settings tab from `settings/+page.svelte` into its own component.
- Collapse `reply`/`reply-all`/`forward` handlers or make them enforce semantics server-side (part of P1-3).
- Parse status/source_mode to enums at the storage boundary; delete string literals.
- Merge duplicate send validators; replace error string-matching with structured `EmailError` variants.
- Separate `reply_to_message_id UUID` column for drafts (ends dual-use `in_reply_to`).
- Normalize OpenAPI path params to `{id}`; move DELETE body to a POST action or query param; regenerate the published contract.
- Pin frontend/backend module registry defaults with a test (they already drift in description text).

### 7.3 Do not refactor

- Double HTML sanitization (ammonia + DOMPurify) — deliberate defense in depth.
- `ImapStream` enum with manual `AsyncRead/Write` — justified single-session abstraction.
- Domain-struct vs response-DTO layering — normal, not duplication.
- Content-addressed `blobs/` storage — correct design; only missing lifecycle (see P2-11 + Future-8). Do not bolt on naive deletes.
- `Mail*` event-sourcing — deliberate audit design; only payload contents need a privacy decision.

---

## 8. Test-gap plan (audit §17) — implementation order

Current state: read-path, archive-job, and linking suites are genuinely strong (11 archive tests with a mock IMAP session incl. UIDVALIDITY reset, retention, watermark; 7 linking tests; 6 read tests incl. cross-tenant 403 and sanitize e2e). Everything Phase 6/7 added around live-mailbox interaction has no meaningful backend coverage. Frontend covers API shapes + compose/list happy paths; mail e2e is zero. `#[ignore]`-gated DB tests **do** run in CI (`integration-tests.yml:161`, `-- --ignored`) — except `mail_imap_import_test.rs`, which needs a public IMAP host and can never run (P2-24).

| Order | Missing tests | Blocks |
|---|---|---|
| 1 | `ImapOps` trait + mock tests for mark-read/unread, move, archive, trash, delete: happy path, UIDVALIDITY-mismatch abort, no-MOVE refusal, no-UIDPLUS refusal, destination-folder failure | P1-5, R1 |
| 2 | Draft lifecycle DB tests: save/update/discard/send; Bcc round-trip; artifact-folder cleanup on all three paths; failed update keeps old draft; attachment-deleted error | P1-1, P2-12/15/16/27 |
| 3 | Mock-SMTP wire assertions: Message-ID present, In-Reply-To/References with angle brackets, Bcc envelope-only, validation limits, 4xx/5xx failure mapping, Sent-append failure flag | P2-22/26, P3-9/10 |
| 4 | Reply-all recipient reconstruction tests (self-exclusion, dedup) — server-side once P1-3 lands | P1-3 |
| 5 | HTTP handler tests: same-tenant cross-user 403 on messages/accounts/drafts; draft-path validation 400s; pagination bound on `/mail/messages` | P2-2/15 |
| 6 | Selected-import `process_import_job` mock-session tests (UIDVALIDITY fail, dedup skip, partial-row reclaim); import-jobs list route test | P2-9/24 |
| 7 | Frontend: reader page tests (sanitize wiring, reply/forward prefill with quoting, link mutations); mailbox action buttons; send→refresh; Load-more cursor | P1-2/4, P2-18/21 |
| 8 | Security regression: SMTP `tls_mode:none` → 400; remote-image neutralization; event payload contains no subject | P2-4/5 |
| 9 | Search tests when implemented: criteria building, ownership scoping, timeout mapping, clear-restores-folder | P2-1 |
| 10 | Scale: >1k-row list pagination, UID-windowed folder listing against fake session | P2-2/18 |

Also flagged: `MailComposeModal.test.ts:83-84` asserts the optimistic "Saved draft" label (passes even if the save contract breaks) — strengthen with payload assertions; `mail_read_test.rs` duplicates the ~245-line AppState harness instead of using `tests/contracts/common.rs`; only 3 well-formed `.eml` fixtures exist — add malformed/nested-multipart/non-UTF-8 fixtures.

---

## 9. Recommended execution order

Dependency-aware remediation batches. **Do not implement in this task.**

1. **Batch 1 — P1 correctness fixes (no schema changes, small diffs):**
   M-P1-1 Bcc draft persistence → M-P1-2 reply/forward quoting → M-P1-3 reply-all self-exclusion/dedup (frontend helper first, server enforcement optional) → M-P1-4 Move picker (or remove button) → M-P2-26 Message-ID → M-P2-21 send→refresh. *Each is independently shippable; order by user harm.*
2. **Batch 2 — Release-gate tests (depends on nothing; enables everything):**
   R1 `ImapOps` trait → P1-5 op tests → Batch-1 regression tests (Bcc round-trip, quoting, reply-all, Message-ID) → draft lifecycle tests (P2-12/15/16/27 fixes can land with their tests here).
3. **Batch 3 — Reliability:**
   R2 pagination + index + draft SQL filter → P2-3 phantom-id fix → P2-17 send idempotency (draft status claim) → P2-8 ordering fix → P2-18 cursor Load-more → P2-19 duplicate-fetch fix → P2-22 append-failure surfacing → P2-9 import-jobs list route + UI.
4. **Batch 4 — Security/privacy hardening + reader polish:**
   P2-5 SMTP none-mode rejection → P2-4 remote-image blocking + opt-in → P2-13 CID inline images → event-payload subject trim + `last_error` sanitization → P3-17 CSP sandbox header.
5. **Batch 5 — Folder robustness + settings cleanup:**
   P2-6 mUTF-7 decode → P2-7 special-use attributes + missing-folder handling → P3-1/2/3/4 settings UI fixes (R4) → P3-19 folder refresh/persistence/INBOX default.
6. **Batch 6 — Search + distinguishability:**
   P2-1 classic search (imported-mail DB search first — schema is ready; then IMAP SEARCH) → P2-25 draft exclusion + imported markers → P2-10 link target picker → P3-30 naming cleanup.
7. **Batch 7 — Cleanup + scale (post-release safe):**
   R3 dead-endpoint decision → 7.2 refactors → P3-29 N+1/pooling → UID windowing (P2-18 completion) → blob GC design (P2-11/Future-8).

---

## Appendix A — verified-clean checklist (negative space)

Confirmed present and correct, with tests where noted: module gate on all 46 handlers (tested); per-handler auth extraction; ownership/tenant checks on every traced path; encrypted secrets with startup key validation; secrets absent from API/OpenAPI; SSRF pinning IMAP+SMTP; TLS enforcement; double HTML sanitization (tested both layers); Bcc wire safety (lettre source-verified); system-vs-user SMTP separation (admin-gated relay); UIDPLUS-gated expunge; MOVE-capability refusal; per-op UIDVALIDITY guards; attachment View-permission re-checks on send/draft/forward; idempotent import/link operations (tested); archive backoff/cancellation/retention semantics (tested); worker panic isolation + stale-job recovery; 25 MB caps on upload/download/outbound; no TODO/FIXME/debug logging of private content anywhere in mail paths; no RAG/AI coupling.

## Appendix B — audit coverage map

| Audit section | Covered by | Key outcome |
|---|---|---|
| §1 Architecture | §2 above | god-service + god-components; clean boundaries otherwise |
| §2 Settings/accounts | §4.3 (1-5), §5 | solid; UI/backend TLS-mode mismatch |
| §3 Main UI/navigation | §3 matrix, P2-14/20 | mostly-webmail; guard + CTA gaps |
| §4 Folders | P2-6/7, P3-19/20 | functional basics; portability gaps |
| §5 Message list | P2-8/18/19/25, P3-21/22 | ordering + pagination + race issues |
| §6 Reader/MIME | P2-4/13, §5 | sanitization strong; tracking + CID gaps |
| §7 Compose/reply/forward | P1-2/3, P2-21/23, P3-8..15 | prefill correctness is the weak layer |
| §8 Drafts | P1-1, P2-12/15/16/27 | lifecycle leaks + validation bypass |
| §9 Sent | P2-3/22/26, P3-14 | threading + visibility gaps |
| §10 Mailbox ops | P1-4/5, P2-7 | safe by construction, untested, Move fake |
| §11 Attachments | P2-11/16/23, P3-13 | inbound solid; lifecycle gaps |
| §12 Import/archive/linking | P2-9/10/24, P3-30 | backend strong; surface visibility weak |
| §13 Search | P2-1 | absent at every layer |
| §14 Security | §5 | no P0; 3 P2; minor payload-privacy items |
| §15 Reliability/perf | §6 | solid worker machinery; list-path scaling gaps |
| §16 Code quality | §7 | 4 must-fix items; god-objects scheduled |
| §17 Tests | §8 | strong where seams exist; none where they don't |
