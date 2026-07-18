# RustShare Mail Final Validation

- **Date:** 2026-07-17
- **Branch:** `audit/mail-webclient` @ `dc893920` ("Address Codex mail review findings")
- **Type:** Independent post-remediation validation. No features added, no refactoring, no defects fixed, no RAG/AI work started.
- **Method:** Read the three prior documents (complete audit, remediation plan, remediation result) as *unverified claims*. Re-verified the code directly via six independent read-only area audits (backend security/isolation; IMAP/folders/list/search/import; compose/drafts/sent/attachments; frontend UI; non-mail regression; cleanup/dead-code), with headline claims spot-checked first-hand by the lead validator against cited lines. Executed the repository's real backend/frontend validation commands, plus the DB/S3-gated mail integration suites against an isolated scratch PostgreSQL 16 database and a scratch bucket on the local RustFS instance. Manual live-provider flow assessed for feasibility (blocked — see below).

---

## Verdict

**Approved with non-blocking follow-ups**

The release standard is met: core flows are implemented and verified in code and by executed tests; credentials are encrypted and never exposed; tenant/user isolation is uniform and tested; HTML is double-sanitized with remote content blocked; destructive IMAP operations are capability-gated and confirmation-gated; user mail never touches system SMTP; drafts and sent behavior are reliable (idempotent sends, create-before-delete drafts, surfaced append failures); lists are paginated and bounded; errors are mapped and understandable; frontend and backend build; meaningful tests pass. No P0 and no P1 findings remain.

Follow-ups that should land before or shortly after tagging (none block Public Preview): a mis-wired archive/trash/read-toggle button cluster in the mailbox UI (P2), the undecided live admin relay `POST /api/v1/mail/send` (P2), user-facing dead folder-mapping settings UI (P2), a stale published OpenAPI contract with a failing freshness test (P2), and a self-starving object-GC queue (P2). The live-provider acceptance flow still has not been executed end-to-end against a real mailbox — it remains required before *general availability*, and is environment-blocked here, not a code finding.

---

## Core flow results

Manual live-IMAP/SMTP execution was **environment-blocked**: no dedicated external IMAP/SMTP test account exists, the SSRF guard correctly rejects loopback/private IMAP hosts (so no local fake server is possible in the release build), and the running deployment's 9 configured accounts are real user data that a read-only validation must not use. Each flow was therefore verified by (a) tracing the current code end-to-end and (b) executing the automated suites that exercise it, including 60 DB/S3-gated integration tests this validation ran itself. Steps marked "live-blocked" need one run with a real provider account before GA.

| # | Flow | Pass/fail | Evidence | Notes |
|---|---|---|---|---|
| 1 | Configure account | Pass (code) | Create/update reject plaintext + STARTTLS IMAP (`mail_service.rs:970-980,1110`); creds AES-256-GCM (`secret_encryption.rs:68-75`); SSRF pin (`imap_client.rs:129-158`) | live-blocked |
| 2 | Test IMAP | Pass (code) | `connect_and_login` with 30s timeouts, auth vs connection error split (`imap_client.rs:74-89,243`) | live-blocked |
| 3 | Test SMTP | Pass (executed) | Mock-SMTP wire tests executed: 10/10 in `mail_smtp_send_test.rs`; 30s `SMTP_TIMEOUT` (`email_service.rs:19,355`) | live-blocked against a real relay |
| 4 | Browse Inbox | Pass (code) | Folder LIST with mUTF-7 decode + RFC 6154 roles (`imap_client.rs:258-272,550-595`); unit-tested | |
| 5 | Paginate | Pass (code) | UID-window cursor + `next_cursor` (`imap_client.rs:297-303`; `mail.rs:1145-1155`); frontend consumes cursor (`MailModuleView.svelte:480-495`); imported list keyset-paginated (`metadata.rs:737-775`) | no `has_more` flag (P3) |
| 6 | Open plain-text message | Pass (executed) | `mail_read_test.rs` parts/source/attachments tests executed (DB-gated) | |
| 7 | Open HTML message | Pass (executed) | Server ammonia strip of remote img incl. `//` (`mail.rs:1584-1605`) + client DOMPurify (`security.ts:12-84`); sanitizer tests executed both ends | no "load remote content" opt-in exists (P3) |
| 8 | Download attachment | Pass (executed) | Owner-scoped attachment endpoint; safe `Content-Disposition` (`public_shares.rs:108-129`); read tests executed | |
| 9 | Mark unread/read | Pass (executed) | Mock-session op tests (`mail_service.rs:3757-3965`); UIDVALIDITY guard; frontend passes `uidvalidity` (`MailModuleView.svelte:816-822`) | |
| 10 | Move/archive/trash | **Partial — P2** | Backend: explicit destination required, MOVE-capability refusal, tested. Frontend: button gating/titles rotated (`MailModuleView.svelte:802-878` — read/unread disabled by `!archiveFolder()`; Archive button gated/titled on `trashFolder()`; Trash button ungated) | works on servers exposing both Archive+Trash; mis-disables/400s otherwise; no data risk (backend validates) |
| 11 | Compose and send | Pass (executed) | Idempotency-key single delivery, preflight-then-claim, Message-ID generated — all executed in `mail_smtp_send_test.rs` | |
| 12 | Reply | Pass (code+unit) | Quoting helpers (`compose.ts:3-21`, `compose.test.ts`); In-Reply-To set | header lacks angle brackets (P3) |
| 13 | Reply all | Pass (unit) | Server-authoritative self-removal + case-insensitive dedup (`mail.rs:2004-2024,2258`; test `reply_all_removes_authoritative_sender_and_duplicates`) | |
| 14 | Forward | Pass (unit) | Body included; unpromoted-attachment warning (`+page.svelte:253-264`) | attachment race + forward-sent-as-reply flag persist (P3) |
| 15 | Save/reopen/send draft | Pass (unit) | Bcc preserved in draft EML (`email_service.rs:459-483`); create-before-delete replace (`mail_service.rs:3326-3387`); artifact cleanup on update/send/discard; full validation on send; per-draft advisory lock | draft-lifecycle DB integration tests absent (unit-only) |
| 16 | Open Sent | Pass (executed) | `append_failed` + `stored` flags in response (`mail.rs:1949-1962`); warning toast (`MailModuleView.svelte:286-293`); phantom-id path removed | sent copy stores `bcc_addresses: []` (P3) |
| 17 | Search | Pass (unit) | IMAP `TEXT` with escaping (`imap_client.rs:542-548`); owner-scoped ILIKE DB search in paginated query (`metadata.rs:757-760`); frontend wired with clear-restore (`MailModuleView.svelte:725-748`) | |
| 18 | Import into RustShare | Pass (executed) | `mail_import_test.rs`, `mail_archive_job_test.rs` executed (dedup, UIDVALIDITY, retention, reclaim); deterministic selected-import tests (`mail_service.rs:3734-3755`) | live-provider import live-blocked |
| 19 | Open imported artifact | Pass (executed) | `/Workspace/Mail` artifact tree; owner+tenant-checked parts/source endpoints (`mail_service.rs:552-568`); read tests executed | |
| 20 | Cross-user access denial | Pass (executed) | Cross-tenant 403 HTTP test (`mail_read_test.rs:543-577`); same-tenant cross-user SMTP-settings denial (`mail_smtp_send_test.rs:107-111`); link-permission denial (`mail_linking_test.rs:101-138`) — all executed | every query owner/tenant-scoped; uniform 404 for foreign accounts |
| 21 | Browser console review | Blocked | No browser in this environment; instead: `npm run check`/`lint` clean (0 errors), 902 unit/component tests pass, no `console.log` in mail paths | |
| 22 | Backend log secret review | Pass (executed) | Running backend logs scanned: 0 password/passwd/secret matches in 1,854 lines; all 34 mail-path `tracing::` sites log ids/counts/hosts only (grep-verified) | running image predates HEAD by ~4 min (23:44 vs 23:48) — read-only probes only |

Validation-area summary (1–20 from the charter): all areas verified; findings are consolidated in **Remaining findings** below.

---

## Security results

- **Secrets — clean.** AES-256-GCM with per-value random nonce for IMAP/SMTP passwords (`secret_encryption.rs:68-75`); startup rejects weak/malformed `RUSTSHARE_SECRET_ENCRYPTION_KEY` (`bootstrap.rs:520-529`); no password field in any response DTO (`mail.rs:74-87,1912-1931`) or in the OpenAPI schema (`password` appears only in *request* DTOs; admin shows `"***"`); `#[serde(skip_serializing)]` on domain secrets; zero secrets in logs (executed log scan + grep of all tracing sites). Plaintext user SMTP rejected at handler and service (`mail.rs:1905-1910`; `mail_service.rs:2694-2698`); the SMTP UI no longer offers it.
- **Tenant isolation — clean.** 47/47 mail handlers take `AuthenticatedUser` + `require_mail_enabled` (counted); all mail SQL filters `tenant_id + owner_id` including parts/attachments owner joins (`metadata.rs:437-443,504-505,726,755`); executed cross-tenant 403 test.
- **User isolation — clean.** Same-tenant cross-user denial executed at service level; foreign accounts/messages return uniform 404; residual: 403-vs-404 existence oracle on messages behind unguessable UUIDs (P3).
- **HTML safety — clean.** Double sanitization (server ammonia with scheme allowlist + attribute filter stripping remote/protocol-relative img src; client DOMPurify forbidding script/style/iframe/on*). CID images rewritten to authenticated `/api/v1/files/{id}/preview` before sanitization (`mail.rs:1607-1619`) with a CSP sandbox header on the part response. Sanitizer tests executed on both ends. Tracking pixels blocked by default; no opt-in control exists (P3).
- **Attachments — clean.** View-permission re-check per file on send/draft/forward (`mail_service.rs:2916-2925,3251-3260`); missing backing file now *fails* the send with a clear message instead of silently dropping; 25 MB caps on upload/import/outbound; safe download filenames.
- **SMTP separation — clean.** All user sends go exclusively through per-user SMTP settings with no system-config fallback (`mail_service.rs:2888-2903` → `email_service.rs:207`); From pinned to the account identity at settings-write and at send; system relay admin-gated (`mail.rs:863-867`). No path exists where user mail uses system SMTP.
- **Destructive actions — clean.** UID-scoped expunge only, UIDPLUS-gated refusal; MOVE refused without capability (no copy+delete fallback); per-op UIDVALIDITY guard; permanent delete behind `confirm()`; draft discard has no confirm (P3). Residual: the guard key is optional server-side — the UI always sends it, API clients may omit (P3).
- **Audit-event privacy — one overstatement.** Per-user send events carry ids/counts only, but the remediation claim "send subjects removed from audit payloads" is **not fully true**: the admin relay still emits `MailMessageSentPayload.subject` (`handlers/mail.rs:1864-1870`; field at `events/types.rs:621`). Exposure is narrow (actor's own event stream; admin-only path) — P3. Raw provider errors still persisted to `last_error` and shown to the owning user only (P3, previously accepted).

No P0. No credential exposure, no cross-user/tenant access, no unsafe HTML execution, no silent data loss, no accidental permanent deletion, no system-SMTP use for user mail.

---

## Test results

**Executed by this validation (real commands, real outcomes):**

| Command | Outcome |
|---|---|
| `cd backend && cargo fmt --check` | PASS |
| `cd backend && SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings` | PASS (0 warnings) |
| `cd backend && SQLX_OFFLINE=true cargo test --all-features --lib` | **734 passed, 0 failed, 31 ignored** (executed) |
| `cd backend && SQLX_OFFLINE=true cargo build --release --all-features` | PASS (3m 07s) |
| `SQLX_OFFLINE=true cargo check --workspace` (root) | PASS |
| `SQLX_OFFLINE=true cargo test --workspace --lib` (root) | **809 passed, 0 failed, 31 ignored** (executed) |
| `cd backend && SQLX_OFFLINE=true cargo test --all-features --test openapi_export_test` | **1 passed, 1 FAILED** — `openapi_spec_is_fresh` (`openapi_export_test.rs:49`) |
| `cd backend && SQLX_OFFLINE=true cargo test --all-features --test mail_smtp_send_test --test mail_read_test --test mail_linking_test --test mail_import_test --test mail_archive_job_test -- --ignored --test-threads=1` (scratch PostgreSQL 16 + scratch RustFS bucket) | **60 passed, 0 failed** — executed DB/S3-gated behavioral tests incl. SMTP wire assertions (Message-ID, idempotent single delivery, unauthorized-From rejection, preflight-then-retry), cross-tenant 403, link permissions, import/archive lifecycle |
| `cd backend && cargo sqlx migrate run` (scratch DB) | PASS — all migrations incl. `20260716000000`, `20260717000000` apply cleanly |
| `cd backend && cargo sqlx prepare --workspace --check` (scratch DB) | PASS (informational "potentially unused queries" warning, same as remediation noted) |
| `cd frontend && npm run check` | PASS — 0 errors, 79 pre-existing warnings |
| `cd frontend && npm run lint` | PASS — 0 errors, 162 pre-existing warnings |
| `cd frontend && npm run test` | **86 files, 902 passed, 5 skipped** |
| `cd frontend && npm run build` | PASS (adapter-static) |

**Tests only compiled:** none counted as passed — everything above executed.

**Skipped:** 5 frontend tests (pre-existing skips); 31 backend lib tests are `#[ignore]`-gated (DB/S3-dependent: `metadata`, `event_store`, `object_store`, repositories, device-auth, sync-handler, and 3 mail-service DB tests: 2 link tests + `import_eml_sets_folder_id_when_folder_creation_succeeds`). The DB/S3-gated *mail* suites were executed separately via `--ignored` (60 passed, above).

**Environment-blocked:** `mail_imap_import_test.rs` — requires a public IMAP host (`IMAP_TEST_HOST`); SSRF correctly prevents loopback, so it can never run locally. Live-provider 25-step acceptance flow — no external test account.

**Failures:** `openapi_spec_is_fresh` — the committed `docs/contracts/rustshare-api-openapi.json` is stale (predates remediation; missing `/api/v1/mail/import-jobs`, `next_cursor*`, `stored`, `append_failed`, `idempotency_key`, search params). This failing test is not covered by the AGENTS.md validation commands and no CI job runs it.

**Flaky:** none observed. Backend lib suite ran twice with identical results; all other suites ran once.

**Coverage gaps acknowledged (tests exist but shallow):** no DB integration tests for draft save/update/discard/send (unit-only); mailbox action buttons (the P2 wiring bug) untested on the frontend; the "Saved draft" assertion still passes unconditionally (`MailComposeModal.test.ts:86-87` asserts an optimistic label with a mocked `onSave`).

---

## Remaining findings

**P0 — none.**

**P1 — none.**

**P2 (fix before/soon after tag; none block Public Preview):**

1. **Mailbox action-button wiring rotated** (`frontend/src/lib/components/modules/MailModuleView.svelte:802-878`): read/unread toggle disabled by `!archiveFolder()` and titled "Archive"; Archive button disabled by `!trashFolder()` and titled "Move to trash" (its action correctly archives); Trash button has no disabled/hint and can send `destination_folder: undefined` → backend 400. The remediation claim "unresolved actions are disabled" (M-P2-7) is false as wired. No data loss possible (server validates destination and UIDVALIDITY); common servers with both folders work.
2. **R3 undecided:** `POST /api/v1/mail/send` admin system-relay remains live, documented in OpenAPI, with no UI caller, and its audit payload still contains the subject (see Security). Decide remove-vs-keep; if kept, drop the subject.
3. **R4 not done:** the settings "Folder mapping" section (five disabled, never-persisted inputs) still ships (`settings/+page.svelte:111-116,1785-1806`). The original audit said remove before release.
4. **Stale published OpenAPI contract:** `openapi_spec_is_fresh` fails (executed); the committed JSON predates all remediation; two utoipa annotation defects (`{account_id}` vs route `{id}` on archive-jobs; `GET /mail/messages` params undocumented); no CI wiring for the freshness test.
5. **Object-GC queue starves itself** (`retention.rs:228-249`): content-addressed `blobs/<sha256>` keys are skipped with `continue` but never dequeued; candidates are fetched `ORDER BY not_before LIMIT 100`. Since file/mail deletions enqueue only blob keys, ≥100 stale keys permanently block all later candidates — object GC silently stalls and the queue grows unbounded. Harmless to data (nothing is deleted), but the deferred feature's scaffolding is broken as built.
6. **Mail blobs never physically deleted (documented deferral, M-P2-11):** correct and safe as far as it goes (no data-loss race), but storage/privacy exposure grows monotonically; needs the cross-process writer/GC lease.

**P3 (polish; condensed):**

- Sent local copies always store `bcc_addresses: []` (M-P1-1's "Expected" half unmet — delivery is correct, the owner-visible record is lost).
- `In-Reply-To`/`References` emitted without angle brackets; threading may break with strict servers (P3-9 unfixed).
- Message-page compose cluster: no `draftId` tracking (every save = new draft), `hasSmtp` not passed (guard bypassed), saved forwards sent flagged as replies, forward reads attachments at click time (race) — `+page.svelte:187,232-233,487-502`.
- IMAP TLS selects still offer `starttls`/`none`, which the backend always rejects — guaranteed failed save (with a clear error) (`settings/+page.svelte:1420-1422,1565-1567` vs `mail_service.rs:970-980`).
- No account enable/disable toggle; disabled "default account" placeholder; link picker covers files only (other types still raw UUID).
- Import-jobs list query unbounded (no LIMIT); `next_cursor` returned without `has_more`; ILIKE `%`/`_` wildcards unescaped (self-fuzz only); 403/404 message-existence oracle.
- Raw provider error strings persisted to `last_error` and shown to the owning user (accepted risk, owner-only).
- `in_reply_to` dual-use leaks an internal UUID into API responses; draft `has_attachments` never persisted; `send_draft` takes first text/plain part ignoring `is_body`; hardcoded `is_forward: false`.
- Every body-part fetch appends a `MailMessageViewed` event and a failed append 500s the read (`mail.rs:1685,1702,1800-1804`).
- Crash between SMTP success and idempotency-row completion leaves a permanent `pending` row blocking same-key retry; draft send holds an advisory-lock transaction across network I/O.
- Draft discard has no confirmation; unused `ConfirmModal` import; accessibility gaps (placeholder-only compose inputs, icon-only buttons without accessible names, buttons nested in `<label>`, no modal focus trap).
- Dead code inventory: `ImapSession::copy_message`, `EmailService::build_raw_eml`, `mailApi.moveMessage` + `MailFolderMoveRequest`, `mailApi.sendMessage` + DTOs (R3), dead `GET /mail/accounts/{id}` and `GET/DELETE /mail/archive-jobs/{job_id}` (no client), `MailSourceMode::InboundAddress`, non-Private `MailVisibility` variants, inert imported-list `is_seen`.
- Stored-account `tls_mode` not re-validated at connect (reachable only via non-API DB writes); no attachment indicator in the live mailbox list (needs BODYSTRUCTURE, deferred); N+1 residuals on low-volume paths; `ensure_mail_root_folder` 2 queries/import.
- CHANGELOG accurate but omits the user-visible P1 fixes; frontend reader module guard reads a hardcoded registry constant (backend 403 remains the real gate).

**Regression review (non-mail areas):** Notes/Files/Folders/Workspace navigation/Module enablement/Activity feed/Search-outside-Mail/File-permission logic are **untouched** by the remediation diff (33 files; only shared-file touches audited). Two intentional, low-risk behavioral changes outside mail: (1) the shared `sanitizeHtml` now strips remote `<img>` in notes and public note shares (security-motivated, no data loss); (2) system SMTP notifications now time out at 30s instead of lettre's 60s default. The blob-GC migration adds triggers/indexes on `files`/`file_versions` but is deliberately inert for file data (content-addressed keys are never deleted). System SMTP notification path and file attachment permission logic unchanged.

---

## Release recommendation

Phase 8 / AI / RAG work may safely begin: there are no P0 and no P1 webmail-client findings remaining. The P2 items are scoped, non-security, non-data-loss follow-ups (one UI wiring bug, one API-surface decision, one dead-UI removal, one contract regeneration, one GC-queue fix) that can land in parallel. Before *general availability* (beyond Public Preview), run the live-provider acceptance flow once with a dedicated external IMAP/SMTP account, and regenerate the published OpenAPI contract.

Webmail client approved: AI/RAG phases may begin
