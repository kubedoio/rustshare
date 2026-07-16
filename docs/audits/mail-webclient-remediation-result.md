# RustShare Mail Webclient Remediation Result

## Files changed

- `docs/plans/mail-webclient-remediation-plan.md`
- `backend/crates/core/src/services/email_service.rs`
- `backend/server/src/handlers/mail.rs`
- `backend/server/src/services/imap_client.rs`
- `backend/server/src/services/mail_service.rs`
- `frontend/src/lib/api/mail.ts`
- `frontend/src/lib/editor/adapter/security.ts`
- `frontend/src/lib/editor/adapter/security.test.ts`
- `frontend/src/lib/mail/compose.ts`
- `frontend/src/lib/mail/compose.test.ts`
- `frontend/src/routes/(app)/modules/mail/messages/[messageId]/+page.svelte`
- `frontend/src/lib/components/modules/MailModuleView.svelte`
- `frontend/src/routes/(app)/settings/+page.svelte`

## Findings fixed

- M-P1-1: draft EML now preserves Bcc for later parsing while SMTP-formatted mail still omits Bcc headers.
- M-P1-2: reply and forward prefill now includes the original body text.
- M-P1-3: reply-all deduplicates recipients, excludes the known account username on the frontend, and removes the server-authoritative SMTP From address in the backend reply-all handler.
- M-P1-4: removed the fake Move button that hardcoded Archive as the destination.
- M-P1-5: mailbox mutations now route through a shared testable IMAP mutation helper with mock coverage for read/unread, move, archive, trash, delete, UIDVALIDITY mismatch, missing MOVE, and missing UIDPLUS.
- M-P2-4: backend and frontend HTML sanitizers now remove remote `http`/`https` image sources, while backend HTML part responses also include `Content-Security-Policy: sandbox`.
- M-P2-5: per-user SMTP `tls_mode: none` is rejected by request validation and service validation; the SMTP settings UI no longer offers plaintext.
- M-P2-8: mailbox folder summaries are served newest-first by UID.
- M-P2-14: the empty Mail CTA now opens the existing mail import picker instead of navigating to Files.
- M-P2-15: outbound service validation now runs before SMTP, including draft-send paths.
- M-P2-16: draft send fails before SMTP when a stored draft attachment has no backing file id.
- M-P2-18: mailbox Load more now uses a server-provided `next_cursor` and appends one UID page instead of refetching an ever-growing limit.
- M-P2-25: general imported-mail lists now exclude draft rows; drafts remain available through the draft workflow.

## Findings partially fixed

- None among the mandatory blocker set addressed in this pass.

## Findings deferred

- Search, imported-list SQL pagination/indexing, idempotency keys, Sent append warning surfacing, import job routing, folder special-use handling, CID images, and blob lifecycle remain as listed in the plan.
- R2/R3/R4 refactors remain open except the fake Move control removal.

## Refactors completed

- Extracted mail compose text helpers into `frontend/src/lib/mail/compose.ts` to cover reply/forward text construction and reply-all recipient deduplication.
- Extracted outbound mail validation into a private backend service helper to cover direct sends and draft sends consistently.
- Added a small `ImapMailboxSession` boundary and shared mailbox mutation helper instead of a larger IMAP service split.

## Cleanup completed

- Removed the misleading Move action from the mailbox message row.
- Changed the empty Mail CTA to reuse the existing import file input.
- No migrations were renamed or edited.
- No RAG, AI, semantic search, embeddings, summarization, or company-memory retrieval was implemented.

## Tests added

- `outbound_draft_message_preserves_bcc_header`
- `outbound_mail_validation_accepts_normal_message`
- `outbound_mail_validation_rejects_invalid_draft_send`
- `mailbox_mutation_rejects_uidvalidity_mismatch_before_mutating`
- `mailbox_mutation_marks_read_and_unread`
- `mailbox_mutation_rejects_move_without_move_capability`
- `mailbox_mutation_moves_to_archive_or_trash_destination`
- `mailbox_mutation_rejects_delete_without_uidplus`
- `mailbox_mutation_deletes_with_uidplus`
- `smtp_settings_request_rejects_plaintext_tls_mode`
- `reply_all_removes_authoritative_sender_and_duplicates`
- `draft_attachment_file_ids_preserves_backing_files`
- `draft_attachment_file_ids_rejects_missing_backing_file`
- `sanitize_email_html_blocks_remote_images`
- `imported_mail_list_excludes_drafts`
- `frontend/src/lib/editor/adapter/security.test.ts` remote image sanitizer coverage
- `frontend/src/lib/mail/compose.test.ts`

## Exact test results

Passed:

- `cd backend && cargo fmt --check`
- `cd backend && SQLX_OFFLINE=true cargo test -p rustshare-core bcc --locked` — 2 passed.
- `cd backend && SQLX_OFFLINE=true cargo test -p rustshare-server mail_service --lib --locked` — 18 passed, 3 ignored.
- `cd backend && SQLX_OFFLINE=true cargo test -p rustshare-server mailbox_mutation --lib --locked` — 6 passed.
- `cd backend && SQLX_OFFLINE=true cargo test -p rustshare-server smtp_settings_request_rejects_plaintext_tls_mode --lib --locked` — 1 passed.
- `cd backend && SQLX_OFFLINE=true cargo test -p rustshare-server reply_all_removes_authoritative_sender_and_duplicates --lib --locked` — 1 passed.
- `cd backend && SQLX_OFFLINE=true cargo test -p rustshare-server draft_attachment_file_ids --lib --locked` — 2 passed.
- `cd backend && SQLX_OFFLINE=true cargo test -p rustshare-server sanitize_email_html --lib --locked` — 2 passed.
- `cd backend && SQLX_OFFLINE=true cargo test -p rustshare-server imported_mail_list_excludes_drafts --lib --locked` — 1 passed.
- `cd backend && SQLX_OFFLINE=true cargo test --all-features --lib` — passed: `rustshare-auth` 12 passed; `rustshare-core` 369 passed; `rustshare-server` 292 passed, 9 ignored; `rustshare-storage` 33 passed, 19 ignored.
- `cd backend && SQLX_OFFLINE=true cargo clippy --all-features -- -D warnings`
- `cd backend && SQLX_OFFLINE=true cargo clippy -p rustshare-server --all-features -- -D warnings`
- `cd frontend && npm run test -- --run src/lib/mail/compose.test.ts` — 3 passed.
- `cd frontend && npm run test -- --run src/lib/mail/compose.test.ts src/lib/api/mail.test.ts` — 12 passed.
- `cd frontend && npm run test -- --run src/lib/editor/adapter/security.test.ts src/lib/mail/compose.test.ts src/lib/api/mail.test.ts` — 27 passed.
- `cd frontend && npm run test -- --run src/lib/editor/adapter/security.test.ts` — 15 passed.
- `cd frontend && npm run test -- --run src/lib/components/modules/MailModuleView.test.ts` — 7 passed.
- `cd frontend && npm run test` — 86 files passed; 900 passed, 5 skipped.
- `cd frontend && npm run check` — 0 errors, 79 existing warnings.
- `cd frontend && npm run lint` — 0 errors, 162 existing warnings.
- `cd frontend && npm run build` — passed.

Failed:

- Initial malformed Cargo command with two test filters; rerun with valid filters passed.
- Initial `rustshare-core bcc` run failed because lettre still omitted Bcc from formatted bytes; fixed with draft-only header insertion and rerun passed.
- Initial `npm run lint` failed on Prettier formatting for `compose.test.ts`; formatted and rerun passed.
- Initial `npm run lint` in this pass failed on Prettier formatting for `MailModuleView.svelte`; formatted and rerun passed.
- Initial full `npm run test` in this pass failed `MailModuleView.test.ts` with `effect_update_depth_exceeded`; replaced mailbox accumulation effect with derived visible state and rerun passed.

Compiled only:

- None reported as passed.

Skipped or not run:

- Full workspace `cargo check` and workspace tests.
- SQLx offline validation.
- Mail integration tests requiring database/object storage.

## Manual test results

Not run in this pass. The full 25-step manual acceptance flow remains required before independent validation.

## Known limitations

- The real Move workflow still needs a folder picker.
- The remediation plan remains the source of truth for the broader audit backlog.

## Remaining follow-up issues

- No mandatory P1 blocker remains open in this report.
- Implement the remaining P2 reliability and privacy items from the plan.
- Regenerate OpenAPI and SQLx metadata when endpoint/schema changes land.

Webmail remediation incomplete: blockers remain
