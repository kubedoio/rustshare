# Elembra Phase F/G0 review findings ledger

This ledger tracks actionable findings and review-discussion disposition for
the Phase F/G0 candidate PRs. It is a live work record, not a release
approval. **Baseline:** RustShare `main` `2078abae9f96dcfc4f62e74c72baba99a63c2c5e`;
Buzz `main` `fe513d015e9885f53118a48519ef1928fb63d5f5`. Entries are updated
only from evidence at the exact source revision listed.

## Findings

### FFG-488-01 — Candidate acceptance image identity mismatch

- **Source:** [PR #488 review thread](https://github.com/kubedoio/rustshare/pull/488#discussion_r4554486863), `docs/pilot/evidence/chat-channel-acceptance-20261111-phase-f.json:36`, head `257f5e0fa7e8649ea54af24174fe3682158bcd53`.
- **Severity / reproducibility:** P2; reproducible by comparing the recorded `imageId` with the candidate OCI index and platform manifests.
- **Root cause / invariant:** Evidence names `sha256:d846651…`, while the pin names OCI index `sha256:38c362bb…`; evidence must identify the exact tested artifact, including index vs platform manifest vs local config/image ID. v1alpha1 compatibility and source provenance must match.
- **Remediation / regression proof:** Resolve registry descriptors and provenance, then rerun required browser acceptance using the exact immutable candidate reference if existing evidence is not sufficient. Capture running container image ID plus requested index and selected platform manifest unambiguously.
- **Current evidence:** Registry trace found that old digest `d846651…` is an amd64-only descriptor whose amd64 child manifest `4bbe7e5a…` equals the amd64 child of the pinned multiarch index `38c362bb…`; source label is `b3aaabf…`. This established content equivalence but was not accepted as proof of an exact-reference run.
- **Fix commit / retest:** Commit `cec833166e8e17266c382ef9eea1737d57532483` adds a fresh exact-index browser run in isolated Compose project `elembra-phaseg0-channel-20261112`: 13/13 PASS, 0 failures. Evidence `docs/pilot/evidence/chat-channel-acceptance-20261112-exact-pin.{json,txt}` records `Config.Image`, Docker `Image`, image `Id`, RepoDigest, index digest, selected linux/amd64 child manifest, config/base digests, source, verified provenance workflow/run and hashes. Temporary stack/volumes were removed and development readiness remained 200. On latest PR head `717977b3`, all required completed checks passed; Chat Product Acceptance and integration both completed successfully.
- **Independent review / discussion:** Independent review confirmed the fresh run used the exact index and that the earlier `d846651…` index resolves to the same amd64 manifest/config, but is not substituted for the exact-index rerun. Commit `717977b3` adds the complete Sigstore bundle/verification result to the repository (SHA-256 `96ca2eadec770c458a1f9962c2064b308a827d5c6dd12559801475d0b00c022a`) and replaces local paths. The exact-head portability review found no remaining finding; the digest thread is resolved. PR remains draft; fresh required human review and deployment approval are still needed; no merge/deploy.

### FFG-488-02 — Provenance verification record was not portable

- **Source:** Independent review of PR #488 evidence at exact head `cec833166e8e17266c382ef9eea1737d57532483`; `docs/pilot/evidence/chat-channel-acceptance-20261112-exact-pin.json` referenced `/srv/data02/projects/elembra-phase-f-evidence/provenance.json`.
- **Severity / invariant:** P2 evidence reproducibility; reviewers must be able to inspect the signature bundle and verification result without access to the build host.
- **Root cause:** The report named a local artifact rather than committing the public Sigstore evidence.
- **Remediation / regression proof:** Signed commit `717977b3` adds `docs/pilot/evidence/buzz-candidate-provenance.json`; JSON assertions confirm the verified subject is OCI index `38c362bb…`, workflow/source SHA `b3aaabf…`, and exact GitHub workflow identity. Relevant PR evidence no longer points to the local file.
- **Independent review / discussion:** Independent review at exact head `717977b3` verified the DSSE signature, exact subject/source/workflow identity, Rekor inclusion, and absence of host-local paths or secrets. The associated digest thread was answered and resolved with the updated evidence. No remaining technical dispute is open; PR approval remains separately required.

### FFG-489-01 — Readiness did not check Chat relay or observer

- **Source:** [PR #489 review thread](https://github.com/kubedoio/rustshare/pull/489#discussion_r4556131289), `backend/server/src/handlers/health.rs` and the original `docs/pilot/phase-g0-operations-handoff.md:16`, original PR head `bfd8965340a53f22dce6f7e5b78d52176d4d3bf5`.
- **Severity / reproducibility:** P2; verified by code inspection: `/health/ready` gates only DB, object storage, event delivery, and auth sessions.
- **Root cause / invariant:** The readiness document claimed aggregate coverage of Buzz and observer without checks. Operators need independent service availability signals; no content, tokens, or signing keys may enter metrics.
- **Remediation:** PR #489 commits `07a673d8` and `1f169db2` add opt-in file-SD probes: observer `/ready` over HTTP and relay TCP availability; empty default targets avoid false alerts for non-Chat deployments. AlertManager down/delivery-failure alerts and an isolated receiver test were added. Targets use `backend:8091` and `backend:7447` because dogfood containers share `backend`'s network namespace.
- **Regression proof:** `promtool check config`; Compose base/Chat overlay validation; `scripts/test-monitoring-integration.py` failure-injected HTTP/TCP success, failure, recovery, AlertManager receiver failure metric, and subsequent isolated delivery. Local tests and PR CI passed at `1f169db227e435d01cf44ddd5972d27a81aa7305`.
- **Independent review / discussion:** A separate agent review of head `1f169db…` found no correctness issue and confirmed namespace aliases, no-chat behavior, and failure-injection coverage. Human reviewer `senolcolak` approved exact head `1f169db…`; the addressed, outdated bot thread was resolved after independent review and CI passed.
- **Remaining boundary:** TCP listener probe does not qualify public NIP-11, Host-derived community, NIP-42, or browser WebSocket. Real pilot receiver and host-resource/backup-heartbeat monitoring remain operational gates.

### FFG-490-01 — Chat restore omitted the explicit Compose override

- **Source:** `scripts/restore-stack.sh` `compose()` versus `scripts/backup-stack.sh` `compose()` at RustShare `main` `2078abae9f96dcfc4f62e74c72baba99a63c2c5e`.
- **Severity / reproducibility:** P1 recovery correctness; code-path comparison reproduces the missing arguments.
- **Root cause / invariant:** Both Chat backup and restore already include `docker-compose.alpha.yml` and `docker-compose.dogfood.yml`, but restore omitted `ELEMBRA_COMPOSE_EXTRA_FILE`, which can hold explicitly selected candidate images/configuration. Restore must reproduce the selected immutable relay/backend images before continuity can be asserted.
- **Remediation / regression proof:** Branch `codex/g0-cross-stack-recovery` adds `ELEMBRA_COMPOSE_EXTRA_FILE` to Chat restore at commit `943370f8` and a fake-Docker regression test that checks every Compose invocation preserves dogfood plus the override. `scripts/test-restore-stack-compose.sh` and `scripts/test-restore-drill-safety.sh` pass locally.
- **Fix commit / retest / independent review:** Commits `943370f8` and `81415b5f` are present; all three restore/recovery safety tests pass locally. Current implementation is in PR #493 at `af94bf44`; full CI and human review pass. The full local drill is blocked by containerd capacity as described in FFG-490-02.
- **Discussion:** No GitHub inline thread; tracked from the requested recovery audit.

### FFG-490-02 — No end-to-end Chat/relay restore continuity fixture

- **Source:** [PR #490 evidence](https://github.com/kubedoio/rustshare/pull/490), `scripts/test-elembra-offsite-backup.sh`, updated evidence at `99c335af6510893cd5605f0bd359f434172d4eca`.
- **Severity / reproducibility:** P1 recovery assurance gap; current test passes with synthetic DB rows and object files and starts no Buzz relay.
- **Root cause / invariant:** Existing browser suites do not persist a test-key/event/attachment-hash manifest or invoke backup/restore; live relay conformance removes its one-image volumes. Thus signed history, identities/bindings, permissions, Files continuity, observer replay, and old/new image rollback are not compared across restore.
- **Remediation / regression proof:** Branch `codex/g0-cross-stack-recovery` at `81415b5fe749306962c3064f2750ac0aef0121c5` adds a separate disposable cross-stack fixture. It snapshots signed relay events/authors/channels/membership, Elembra identity bindings/users/observed history/Files metadata and object volumes; restores, compares exact projections and API authorization/attachment bytes, verifies NIP-42 event signatures, restarts observer for replay, then switches immutable old/new relay images while retaining volumes. It uses a unique project and loopback-only ports. Safety tests refuse existing evidence dirs/overwrites and fail closed on teardown residue; they also ensure cleanup failure cannot retain a PASS marker.
- **Fix commit / retest / independent review:** `scripts/test-chat-recovery-drill-safety.sh`, `scripts/test-restore-stack-compose.sh`, and `scripts/test-restore-drill-safety.sh` pass; independent review found no code blocker at `be93b476`. The exact-head drill `elembra-g0-recovery-20261112t224629z-21447` exited during source-build startup because `/srv/data01` containerd storage is full. It did not reach Chat acceptance, backup/restore, relay switching, or observer replay. Cleanup verified no recovery containers, volumes, or networks remain; two temporary image tags remain because containerd metadata writes failed. Do not retry Docker until the host operator frees that filesystem. No recovery PASS is claimed.
- **Discussion:** PR #490 has no inline review thread. PR #490 must not be treated as full recovery qualification.

### FFG-493-01 — Recovery helper can inherit arbitrary Compose project files

- **Source:** Independent review of PR #493 head `d51e72a696e3d4e95bcc70a563413bd1225d300e`, `scripts/run-chat-recovery-drill.sh` calls to `backup-stack.sh` and `restore-stack.sh`.
- **Severity / invariant:** P1 data-safety; recovery operations must not reach a caller-selected Compose file, external volume, or bind mount.
- **Root cause:** Backup/restore helpers intentionally honor `COMPOSE_FILE`; the isolated driver pinned its own direct Compose commands but inherited this variable when invoking the helpers.
- **Fix / regression:** Signed commit `0a86619b` pins `COMPOSE_FILE=docker-compose.yml`, separator `:`, unique `COMPOSE_PROJECT_NAME`, and core/Chat service identifiers immediately before both helper calls. Safety tests seed attacker-controlled values and assert they are overwritten and both call sites use the pin.
- **Retest / independent review:** `bash -n`, `scripts/test-chat-recovery-drill-safety.sh`, `scripts/test-restore-stack-compose.sh`, and `scripts/test-restore-drill-safety.sh` pass. Independent review at `0a86619b` found no issue. The later run on `be93b4767aa92e6cbfa290d95d43261d2d608fcd` exited during source-build startup due to full `/srv/data01` containerd storage. Cleanup verified zero recovery containers, volumes, and networks; two project-tagged temporary images remain because containerd metadata writes failed. No recovery PASS is claimed; no further Docker operation is being attempted on that filesystem.
- **Discussion:** PR #493 reply records the fix; fresh human review requested. No merge/deployment.

### FFG-493-02 — Dogfood requests could target a non-isolated service

- **Source:** PR #493 bot review thread [P1](https://github.com/kubedoio/rustshare/pull/493#discussion_r4557168013), `scripts/run-alpha-dogfood.sh` invoked by the isolated recovery harness.
- **Severity / invariant:** P1 isolation; all state-changing acceptance requests must remain on the disposable loopback deployment.
- **Root cause:** Login, relative mutation, readiness, and relay health URLs were hardcoded; a caller-provided public origin could also override derived defaults.
- **Fix / regression:** Commit `b81c2865` derives API and relay checks from explicit URLs; `be93b476` pins `ELEMBRA_ORIGIN` and `BUZZ_RELAY_HEALTH_URL` to generated loopback endpoints in the harness, overriding caller values. Static tests assert both pins.
- **Retest / independent review:** Independent review at `be93b476` confirmed hostile inherited origins cannot route requests away from the disposable stack. Syntax and safety tests pass; full randomized-port run is blocked by containerd capacity. Thread answered and resolved with this limitation recorded separately.

### FFG-493-03 — Channel evidence path was prefixed incorrectly

- **Source:** PR #493 bot review thread [P1](https://github.com/kubedoio/rustshare/pull/493#discussion_r4557168025), `scripts/run-chat-channel-acceptance.sh` invoked by recovery harness.
- **Severity / invariant:** P1 evidence integrity; the PASS gate must read the exact artifact produced by Playwright.
- **Root cause:** The acceptance runner prefixes `ACCEPTANCE_EVIDENCE` with its repository root, while the harness passed an absolute `/tmp` path and read from the original absolute location.
- **Fix / regression:** Commit `b81c2865` computes a path relative to the isolated worktree and passes it to the runner; safety test asserts the relative-path handoff.
- **Retest / independent review:** Independent review confirmed path semantics. Local syntax/safety tests pass; integrated browser execution is blocked by containerd capacity. Thread answered and resolved.

### FFG-493-04 — Snapshot command failures could hash empty output

- **Source:** PR #493 bot review thread [P2](https://github.com/kubedoio/rustshare/pull/493#discussion_r4557168039), `scripts/run-chat-recovery-drill.sh:snapshot`.
- **Severity / invariant:** P2 recovery integrity; missing/altered database or object data must prevent a PASS.
- **Root cause:** Hash commands were embedded in `printf` substitutions, allowing a failed command’s status to be masked by successful `printf`.
- **Fix / regression:** Commit `b81c2865` checks command output and captures all ten database/object hashes in explicit assignments before writing; missing containers or failed volume inspection fail closed. The safety test verifies a failed command cannot produce a hash.
- **Retest / independent review:** Independent review confirmed failure propagation. Local syntax/safety/restore tests pass; full restore execution remains blocked by containerd capacity. Thread answered and resolved.

### FFG-491-01 — SvelteKit conditional advisories #68–71

- **Source:** [PR #491 alert disposition](https://github.com/kubedoio/rustshare/pull/491), exact main package tree and source scan.
- **Severity / reproducibility:** Low; package `@sveltejs/kit` 2.70.3 is within the advisories' `<3.0.0` range; affected build target is the static frontend build.
- **Root cause / invariant:** GHSA-27mw-82g8-phjj requires untrusted `setHeaders` cache-control during prerender; GHSA-pfhg-q3qv-9q6r requires enhanced forms and redirect to a `//` path. Exact-tree scan found no `setHeaders`, `$app/forms`, or `use:enhance`; static adapter can prerender, so the first advisory condition still merits review.
- **Remediation / test:** Keep alerts open; reviewer to validate route/config-generated sources and decide a condition-based exception or tested SvelteKit major upgrade. No dependency bump or dismissal is supported by current evidence.
- **Fix/retest/review/discussion:** No code fix; human disposition of the conditional advisories remains pending; no PR inline thread. The dated report at `dcfc71d8a0c3d7efee9d87a830d9b26736770a91` includes exact source evidence and was approved by `senolcolak` at that head.

### FFG-491-02 — Affected build-time `deepmerge` alert #80

- **Source:** [PR #491 alert disposition](https://github.com/kubedoio/rustshare/pull/491), `frontend/package-lock.json` at baseline SHA.
- **Severity / reproducibility:** Per GitHub alert; exact lock resolves `deepmerge` 4.3.1, affected range `>=4.2.0 <=4.3.1`, no patched version reported.
- **Root cause / invariant:** Sole root path is dev-only `@sveltejs/vite-plugin-svelte@7.3.1 → deepmerge`, merging repository-local Svelte config and inline Vite options. This is a build-host/supply-chain input risk, not a browser request path.
- **Remediation / test:** Keep alert open; assess trust/CI exposure and update when a fixed plugin/deepmerge release exists. No safe patched version or production dependency bump is available now.
- **Fix/retest/review/discussion:** No code fix; independent owner disposition remains pending; no inline thread. The dated report at `dcfc71d8a0c3d7efee9d87a830d9b26736770a91` includes `npm explain` reachability evidence and was approved by `senolcolak` at that head.

### FFG-CODEQL-01 — Open code-scanning findings

- **Source:** [RustShare Code Scanning alerts](https://github.com/kubedoio/rustshare/security/code-scanning).
- **Status:** Paginated inventory at exact main contains 52 CodeQL and 22 Trivy open findings (74 total). Forty-one CodeQL hard-coded-crypto findings reduce to 39 test fixtures and two uses of the public dummy password with the static dummy password hash for timing equalization; cleartext login timing logs are in an ignored test; cleartext DB finding uses SQLx-bound Argon2 password hashes; search allocation is bounded to 50 results and candidate truncation. These are not production secrets or cleartext credential persistence.
- **Actionable CodeQL source flows:** #311/#312 mail HTML-to-text conversion decoded untrusted markup and fed it to a Tiptap HTML sink; #313 and #315–317 logged untrusted WebSocket event types/full malformed payloads; #314 logged relay/error text to observer stderr. PR #492 at `ec3c5ed5` sanitizes HTML before extraction, serializes plain text through a DOM text node, keeps diagnostics static, and covers literal markup, entity handling and an encoded `&nbsp;`. Independent review found no injection path; the follow-up literal-entity correction is being independently rechecked. Four focused frontend files passed 12/12 at `c25b7be7`; the added mail helper regression passes 4/4 locally. At `7c1119ae`, exact-head CodeQL passed JS and Rust analyses with zero findings. The new exact-head CI is pending. Historical CodeQL records #412/#413 refer to earlier merge commits with results and were not manually dismissed.
- **Trivy:** All 22 current open findings are pilot backend image results, 14 Medium and 8 Low, on the same `libc6 2.41-12+deb13u4`; no fixed package version is reported. #386 is CVE-2026-84243 (Medium) and #294 is CVE-2026-97399 (Low). Trivy 0.70.0 scans of the exact pinned Distroless Debian 13 base index `sha256:e6b0f82595aa1edde72c8054de7a9dca30f0f8f54820c37a167c0ddfc21c5c67` show Debian 13.7 and the same package/CVEs on amd64 and arm64. The current tag resolves to that exact index; no patched upstream digest/version exists to adopt. A full Trivy scan of the provenance-bound amd64 pilot image found 24 records (14 Medium, 8 Low, 2 Unknown); the 22 Medium/Low match the open alerts. Published RustShare image provenance is workflow run 42913463633 at the baseline source and binds archive SHA `b2b724785ac1462c0e0714d517b4606f059f94a1bfefdab72d2f40e4c786267d`, digest `sha256:44f22123ff9ca7a318fa24b1115a07362bdb08047132341d64a64fbb612fa8fe`, image ID `sha256:bfc3b306cffe65a710ae6e0391278a5f20abe6d5cc19d3a43675a20bcf7a9015`, and source SHA `2078abae…`. No rebuild/pin change is justified until upstream publishes a patched base.
- **Fix/retest/independent review/discussion:** No alert was manually dismissed. PR #492’s initial sanitizer triggered CodeQL High alerts #412/#413; independent reviews also found plain-text loss, an entity double-decode, and the literal `&nbsp;` defect. Signed fixes preserve literal content; focused frontend tests pass 12/12 at `c25b7be7` and 4/4 for the new mail helper regression at `ec3c5ed5`. Exact-head frontend, JS/Rust analysis, Chat Product Acceptance and integration all passed at `ec3c5ed5`; Senol approved that exact head. Historical alert records were not manually dismissed. Trivy outcome remains upstream-version review pending.

### FFG-492-01 — Literal `&nbsp;` text was decoded twice in mail replies

- **Source:** Independent review follow-up on PR #492, `frontend/src/lib/mail/compose.ts` and the reply/forward callers in `frontend/src/routes/(app)/apps/mail/messages/[messageId]/+page.svelte` and `frontend/src/lib/components/apps/MailApplicationView.svelte`.
- **Severity / reproducibility:** P2 content correctness; HTML `<p>&amp;nbsp;</p>` is parsed by `DOMParser` to literal `&nbsp;`, then the extra `.replace(/&nbsp;/g, ' ')` erased it. The new test failed before the fix, returning empty text instead of literal `&nbsp;`.
- **Fix / regression:** Signed commit `ec3c5ed5` removes only the redundant entity replacement; actual U+00A0 still normalizes to a space. `frontend/src/lib/mail/compose.test.ts` now asserts the encoded literal remains editor-safe output `&amp;nbsp;`. Focused Vitest passes 4/4, targeted ESLint and Prettier pass.
- **Independent review / discussion:** Follow-up review at exact head `ec3c5ed550f8fd7da5222e36570faf4ac2ed71c2` confirmed the redundant decode is removed, the regression matches parser semantics, and callers remain unchanged; no further finding. The PR comment records test evidence. Full CI passed and Senol approved exact head; no merge/deployment.

## Additional verified code finding

### FFG-CODEQL-02 — Observer runtime image omitted a new imported module

- **Source:** PR #492 `scripts/buzz-observer.mjs` importing `frontend/scripts/observer-log.mjs` and `docker/chat-observer.Dockerfile` COPY list; exact-head Chat Product Acceptance run `43316022502`.
- **Severity / reproducibility:** P1 integration regression; the browser acceptance startup reproduced `ERR_MODULE_NOT_FOUND` and the observer restarted before any scenarios ran.
- **Root cause:** PR #492 added the safe logger module but left it out of the container image file list.
- **Fix / regression:** Signed commit `7c1119ae` adds the module to Docker COPY. Local Docker image build, `node --check` on the entrypoint, and import of the module inside the built image pass.
- **Retest / review:** Local image build and module-import checks passed. On exact head `ec3c5ed5`, JS/Rust analysis, frontend CI, Chat Product Acceptance, and integration passed. Senol approved exact head; the packaging issue is fixed, not dismissed.

### FFG-493-05 — Relay history probe formatting failed CI

- **Source:** Exact-head frontend lint failure on PR #493, run `43324567758`, job `131553656264`, file `frontend/scripts/chat-relay-history-probe.mjs`.
- **Severity / reproducibility:** P2 CI gate; `npm run lint` failed because Prettier rejected the file layout.
- **Fix / regression:** Signed commits `ea9a6d8a` and `af94bf44` corrected formatting; the latter uses the repository settings (`useTabs`, single quotes, no trailing commas, width 100). Prettier check with those settings, `node --check`, and `git diff --check` pass locally. The intermediate exact-head run `43325324367` failed because the first pass used wrong defaults; a fresh CI run is now active at `af94bf443abde2902688e2ad3fd7c090a68d6f80`.
- **Independent review / discussion:** Independent review at exact head `af94bf44` confirmed the proper format and that the commit changes layout only. No review thread was attached to this CI finding.

## Findings discovered in the 2026-11-14 review pass

### FFG-489-02 — Observer could report ready before a successful replay

- **Source:** [PR #489](https://github.com/kubedoio/rustshare/pull/489), original head `1f169db227e435d01cf44ddd5972d27a81aa7305`; affected `frontend/scripts/buzz-observer.mjs` reconciliation and `/ready` state.
- **Severity / reproducibility:** P2 availability and history-integrity defect; an observer in `starting` satisfied the old `observation !== 'degraded'` predicate, replay failures did not always degrade readiness, and per-event success could clear an unrelated replay failure.
- **Root cause / invariant:** Readiness must mean the observer authenticated and completed history reconciliation. Failed or concurrent delivery must not be hidden or skipped by a later replay cursor.
- **Remediation / regression:** Signed commits `0fe49e5b` and `79e9f76a` require `observation === 'ready'`, advance readiness/cursor/recovery timestamp only after a complete generation-stable replay, and make replay failure degrade state. `frontend/scripts/buzz-observer-policy.test.mjs` covers start, success, failure, stale replay preserving cursor/timestamp, and recovery. `.github/workflows/frontend-ci.yml` now runs `npm run test:buzz-config`.
- **Retest / independent review:** `npm run test:buzz-config` 16/16 PASS; `node --check` PASS; `npm run check` PASS (0 errors, 36 pre-existing warnings); `npm run lint` PASS (0 errors, pre-existing warnings). Parent head `0fe49e5b` passed 136 frontend test files / 1,481 tests and production build. Independent review of exact final diff found no remaining state/cursor race. At final commit `79e9f76a`, GitHub CI is in progress; Cargo Deny fails on inherited ammonia until #496 is integrated. Senol re-review requested.
- **Discussion:** The prior readiness coverage concern is addressed; no unresolved inline review thread remains. No merge/deployment.

### FFG-491-03 — Alert inventory omitted the newly opened adapter-static advisory

- **Source:** [PR #491](https://github.com/kubedoio/rustshare/pull/491), report `docs/security/dependabot-alert-disposition-2026-11-12.md`, main baseline `2078abae9f96dcfc4f62e74c72baba99a63c2c5e`.
- **Severity / reproducibility:** Medium supply-chain/build-tool advisory #81, GHSA-jf9x-2fg2-hx4w / CVE-2026-128121; the main lock held affected `@sveltejs/adapter-static` 3.0.10.
- **Root cause / invariant:** The previous triage snapshot predates alert #81/#82 and did not reflect current live record totals. Security dispositions must match exact alert state and dependency graph.
- **Remediation / regression:** Signed report refresh `2a291fd6` records 61 open / 15 auto-dismissed alert records, includes #81 and GitHub-auto-dismissed #82, and makes no manual dismissal. Separate dependency patch [PR #497](https://github.com/kubedoio/rustshare/pull/497) updates adapter-static to 3.0.11.
- **Retest / independent review:** Full local frontend validation on #497: check/lint/build PASS; 136 test files and 1,481 tests PASS. #491 report is documentation-only; refreshed branch checks and independent approval are pending. Advisory remains open until #497 is reviewed/merged and the graph refreshes.
- **Discussion:** No review thread was resolved or dismissed to clear the dashboard; fresh review requested.

### FFG-495-01 — Memory reconcile integration gate deadlocked during discovery

- **Source:** [PR #495](https://github.com/kubedoio/rustshare/pull/495), run `43551107841`, job `132313536085`, test `files_reconcile_worker_automates_recovery_and_bounds_failures` at `backend/tests/memory_reconcile_test.rs:923`; protected base `2078abae9f96dcfc4f62e74c72baba99a63c2c5e`.
- **Severity / reproducibility:** P1 CI/recovery reliability gate. PostgreSQL recorded an actual deadlock before the 10-second assertion; one backend waited for the discovery singleton lock and another on reconciliation-job insertion. It is not classified as a generic flake.
- **Root cause / invariant:** The worker's database statement/lock timeout is 5 seconds, while the test's outer page timeout is 2 seconds. Repeated isolated runs pass but reproduce statement cancellation and `already a transaction in progress` warnings. Available logs do not prove whether cancellation cleanup caused the exact CI deadlock.
- **Remediation / regression:** A focused cancellation-then-discovery recovery regression and timeout alignment are being prepared in a separate worktree; no product change or cause claim until the test proves it.
- **Retest / independent review:** Exact CI integration job failed. Five isolated PostgreSQL/RustFS repetitions passed but reproduced cancellation warnings. Fix/test status and independent review remain pending.
- **Discussion:** No inline thread; PR approval is required and remains requested. Do not merge while the required integration gate fails.

### FFG-496-01 — Cargo Deny blocked by inherited ammonia advisory

- **Source:** [PR #496](https://github.com/kubedoio/rustshare/pull/496), head `4347368e955c4fd21d9db202253841e182d0ac8f`.
- **Severity / reproducibility:** Build/security gate; `ammonia 4.2.4` triggers RUSTSEC-2026-0533 in protected main and therefore blocks otherwise unrelated PR checks.
- **Root cause / invariant:** Dependency advisory remains resolved in the lockfile until a reviewed source update reaches protected main.
- **Remediation / regression:** Dedicated ammonia update in #496; Cargo Deny and all reported CI checks pass on its head.
- **Retest / independent review:** Exact-head CI PASS; approval is present. The PR remains open/unmerged; no automatic merge or alert dismissal.

### FFG-497-01 — adapter-static 3.0.10 remained inside the #81 affected range

- **Source:** [PR #497](https://github.com/kubedoio/rustshare/pull/497), main baseline above, `frontend/package.json` and lockfile.
- **Severity / reproducibility:** Medium build-tool supply-chain issue; the advisory affects versions `<3.0.11`; live main resolves 3.0.10.
- **Root cause / invariant:** Dependabot alert #81 was actionable despite the advisory's Vercel-specific exploit precondition not matching the documented nginx deployment.
- **Remediation / regression:** Minimal package/lock update to 3.0.11 at head `a1c0332ea585978b94e3ff2710e6d79a2987f95b`.

## Execution refresh — 2026-11-14 (supersedes older pending statuses above)

- **PR #493 recovery fixture:** Latest pushed head `b9f70b00209606f8ea16b69bf4dc64c8b46d3d60`. It adds post-restore private-channel authorization checks, authenticated NIP-42 positive and restricted history probes, observer catch-up after publishing while stopped, precise OCI index/platform/config/runtime image identity, backend/observer source labels, and safe private-fixture cleanup. Independent final source review found no new actionable issue. `scripts/test-chat-recovery-drill-safety.sh`, `bash -n`, Node syntax, Prettier, `git diff --check`, and `npm run check` pass (0 errors, 36 pre-existing warnings); read-only Compose config validation confirmed the merged override retains ports and exact source revision args. New CI has passed format, clippy, frontend analysis, migration baseline, and contract checks so far; Cargo Deny fails on inherited `ammonia 4.2.4` until #496 reaches main, while live acceptance/integration checks are still running. Fresh human review is requested. The full Compose recovery drill was not run after these edits. Earlier run/evidence at `941f6f…` is historical and is not evidence for this patch.
- **Recovery capacity gate:** Docker root is `/var/lib/docker` on `/`, with 12 GiB free at the last check; `/srv/data01` is a separate filesystem with 94 GiB free. Docker reports 20.08 GB images, 6.937 GB volumes, and 67.68 GB build cache. No global prune or deletion of unrelated resources is authorized or performed. The end-to-end restore/replay/rollback test remains blocked until the host owner provides safe capacity remediation or another approved host.
- **PR #495 CI regression:** Exact failing required integration job `132313536085` in workflow run `43551107841` deadlocked in `files_reconcile_worker_automates_recovery_and_bounds_failures`; cause is not yet proven. Test-only PR #498 at `aefa7b3a2c089081e27a882f8d37a79d8da530c2` adds cancellation/re-discovery regression and timeout alignment. Independent source review found no defect; local targeted test, existing worker test, fmt, and clippy passed. Exact CI is pending and does not yet prove the original CI deadlock cause fixed.
- **PR #495 release workflow:** Head `41c916b0f441388900bf82213c59cad131bd86dc`; routine main pushes execute smoke/integration only, while image publication requires an explicit `workflow_dispatch`, `publish_image=true`, main ref, and protected `pilot-image-publication` environment. No image publication occurred. Required integration currently fails at the deadlock above, so this PR cannot merge yet.
- **PR #496:** Head `4347368e955c4fd21d9db202253841e182d0ac8f`, review approved and checks pass; it clears inherited `ammonia 4.2.4` Cargo Deny failure. It remains unmerged until the mainline image-publication workflow is safe and its required gates pass.
- **PR #489:** Head `79e9f76ada5fc6d80d869b7939907804b22a93b8`; focused observer tests 16/16, syntax, Svelte check (0 errors; 36 pre-existing warnings), and lint pass. Latest PR check has inherited Cargo Deny failure pending #496; fresh human review is requested. The full frontend suite/build passed on the parent revision, not this exact head.
- **PR #491 / #497:** Refreshed alert inventory head `2a291fd6874b10fb4b78398c459ca91b99e8e67c`; 61 open and 15 auto-dismissed alerts, with no manual dismissal. #81 is patched by #497 (`a1c0332ea585978b94e3ff2710e6d79a2987f95b`) from adapter-static 3.0.10 to 3.0.11; local frontend check/lint/build and 1,481 tests pass. Fresh review and CI remain pending; inherited Cargo Deny failure is #496's ammonia advisory. #491 and #497 remain open.
- **Other current PR heads:** #488 `717977b31c62041918dc1599e47488e4de12cbce` remains draft and unmerged pending off-host recovery and operator/pilot gates. #490 `99c335af6510893cd5605f0bd359f434172d4eca` remains open; its synthetic backup drill is not full Chat recovery. #492 `613b4a07f765ae5a1a1bfe9192ca6b158542a22f` is approved with checks passing but unmerged. #494 is this ledger update. No PR was merged, no image was published, no deployment was performed, and no alert was manually dismissed in this pass.
- **Current decision:** Code-review clean: **not established**. Security review clean: **not established**. Operational qualification: **incomplete**. Institutional pilot: **NO-GO** until exact-head recovery, required checks/reviews, off-host recovery and operator-host gates are satisfied. PR #335 host/operator dependencies and #460 managed-signing/upstream-publisher stop conditions remain external gates.
- **Retest / independent review:** `npm ci --ignore-scripts`, check, lint, 136 frontend test files / 1,481 tests, and production build pass locally. GitHub CI is incomplete; Cargo Deny inherits ammonia until #496 reaches main. Independent human review requested.

## Current gate state — refreshed 2026-11-14

- **Protected main:** RustShare remains `2078abae9f96dcfc4f62e74c72baba99a63c2c5e`; Buzz remains `fe513d015e9885f53118a48519ef1928fb63d5f5`. No PR was merged, no image was published, and no deployment occurred.
- **#488:** exact head `717977b31c62041918dc1599e47488e4de12cbce`; approved and required checks passed. Exact OCI-index acceptance and provenance are independently verified, v1alpha1 is retained, and the digest thread is resolved. Converted to draft because the PR changes the canonical compatibility pin while off-host recovery, operator authorization, and real pilot-host gates remain incomplete.
- **#489:** exact head `79e9f76ada5fc6d80d869b7939907804b22a93b8`. Observer readiness now requires successful reconciliation; stale reconciliation cannot clear a newer delivery failure or advance its replay cursor/recovery timestamp. Focused `test:buzz-config` passes 16/16, `node --check`, frontend type check (0 errors; 36 existing warnings), and lint (0 errors; existing warnings). Earlier full frontend validation at parent `0fe49e5b` passed 136 test files/1,481 tests and production build. Latest exact-head CI is still running; Cargo Deny fails on inherited `ammonia 4.2.4` / RUSTSEC-2026-0533, with the focused fix in #496. Human re-review requested; no unresolved inline thread.
- **#490:** exact head `99c335af6510893cd5605f0bd359f434172d4eca` accurately describes the synthetic local backup pipeline. It is not full Chat recovery. Cross-stack proof is in #493, not this PR.
- **#491:** exact head `2a291fd6874b10fb4b78398c459ca91b99e8e67c`; refreshed report records 61 open Dependabot alerts (2 Critical, 18 High, 32 Medium, 9 Low), 15 GitHub-auto-dismissed records, no manual dismissals, and the new #81/#82 records. Human review requested; Cargo Deny currently fails on the inherited ammonia advisory. All 3 true High CodeQL source issues on main have a reviewed fix in #492 but remain present on main until merge/rescan.
- **#492:** exact head `613b4a07f765ae5a1a1bfe9192ca6b158542a22f`; approved, CodeQL and required checks pass. It fixes the mainline mail sanitizer and diagnostic-log findings. No merge was performed.
- **#493:** current remote head `941f6fefa6bc53497c5c3ef1a2ed76b5d7f964cc`; review required. A private local full-stack drill for that exact head reported PASS (13/13 channels, 9/9 conversations, 26 signed relay events and observer rows, Files checksum/authorization, restore, relay image switch/rollback). Independent audit found it did not prove observer catch-up after a new event during outage, did not repeat private-history/removed-member denial after restore, and mislabeled the OCI index as runtime image identity without the platform child. A focused harness patch is being independently reviewed and rerun; no new exact-head PASS is claimed yet. Earlier containerd capacity failure was cleared; `/srv/data01` had 94 GB free at the last independent audit. Off-host recovery, independent key custody, and browser-held key recovery remain untested external gates.
- **#494:** this findings ledger is at `6fe11d68e3a832fd747935c2008f9d203e028a92`; this refresh is new work and requires fresh checks/review after push.
- **#495:** exact head `41c916b0f441388900bf82213c59cad131bd86dc`; review required. Workflow guard is correct: push-to-main runs smoke only; publication requires `workflow_dispatch` with `publish_image=true`, `refs/heads/main`, and protected environment `pilot-image-publication` requiring Senol’s approval and protected branches. The exact run has a failing integration test: PostgreSQL logged a real deadlock in `files_reconcile_worker_automates_recovery_and_bounds_failures`, before its timeout assertion. Five fresh isolated repetitions pass but reproducibly log a server-side statement cancellation and nested-transaction warning; exact deadlock root cause remains unproven. A cancellation-recovery regression/test-timeout correction is being prepared. Do not treat the failure as a generic flake.
- **#496:** exact head `4347368e955c4fd21d9db202253841e182d0ac8f`; the ammonia update clears Cargo Deny and all reported CI checks pass. It has approval, but was not merged; it is a separate security/dependency change and protected-main integration remains a human-controlled gate.
- **#497:** exact head `a1c0332ea585978b94e3ff2710e6d79a2987f95b`; updates adapter-static to patched `3.0.11` for Dependabot #81. Local full frontend check/lint/test/build passed (136 files, 1,481 tests). Independent review is requested; CI is incomplete and Cargo Deny inherits the ammonia failure. Alert #81 remains open pending review, merge, and graph refresh.
- **Review discussions:** GraphQL reports no unresolved inline review threads on #488–#497 at this snapshot. New findings above are recorded separately and must not be marked “resolved” until current-head CI and required human review complete.
- **Operational gates:** issue #335 still requires the real protected FWS host, second operator/two-admin recovery rehearsal, identity/SSO config, off-host backup target with independently held keys, alert destination, and rollback authorization. Issue #460 continues to block upstream publisher/managed signing. No pilot GO is claimed.

## Review loop refresh — current exact heads, 2026-11-14

- **PR #485:** merged as `cd5d752bae5eae6643b7b6a00930d53b6642c1c3`. Its CodeQL #410 alert is fixed on the merge ref, not dismissed. The community-isolation and transport/empty-read discussions were answered and resolved; the CodeQL thread was answered with the live alert state and fix behavior. No new #485 code change is indicated.
- **PR #489:** current head `7eb48466e3ca22e2567eb10d4ee40209ca41320a`. Fresh review found replay could start on WebSocket open before NIP-42 authentication; the post-auth replay could then be dropped by the `reconcileRunning` guard. Replay now requires connected + authenticated + selected-community state and starts after authenticated subscription setup. Regression coverage is in `frontend/scripts/buzz-observer-policy.test.mjs`. Local exact-head checks: `npm run test:buzz-config` 17/17; `npm run check` (0 errors); `npm run lint` (0 errors); `npm run test` (136 files / 1,481 tests); `npm run build`; and `git show --check` all pass. GitHub exact-head integration, Chat Product Acceptance, and Rust CodeQL were still running at refresh; Cargo Deny fails only on the inherited `ammonia 4.2.4` / RUSTSEC-2026-0533 issue pending #496. Review approval is required again for this new head.
- **PR #493:** current head `b9f70b00209606f8ea16b69bf4dc64c8b46d3d60`. Required integration, Chat Product Acceptance, relay conformance, CodeQL, format, clippy, and contract checks pass; Cargo Deny fails on the same inherited ammonia advisory. The full Compose recovery drill was not rerun for this exact head, so no full recovery PASS is claimed. The earlier drill at `941f6f…` is historical and does not qualify the current head.
- **PR #495:** current workflow change head `41c916b0f441388900bf82213c59cad131bd86dc`. Main pushes do not publish images; publication remains restricted to explicit dispatch plus the protected environment. Its exact integration job failed in `files_reconcile_worker_automates_recovery_and_bounds_failures` with a PostgreSQL deadlock. PR #498's cancellation regression and timeout alignment pass on its own head, but do not turn #495's failed exact-head check into a pass; preserve the merge gate until the failure is proven resolved and the relevant checks rerun.
- **PR #498:** current head `aefa7b3a2c089081e27a882f8d37a79d8da530c2`; exact integration and Chat Product Acceptance pass. The test targets compile and its focused local regression/worker tests pass. Cargo Deny inherits the ammonia advisory. No newly found code issue in this review.
- **Buzz PR #7:** reviewed current head `dc09fc2602a7cc64ea15caa865bd773d8be61eed`. Staging and release serialize on the community row; event writes and live fanout both require active membership with `send_enabled`. `cargo fmt --all -- --check`, both affected integration-test targets with `--no-run`, and targeted Clippy with `-D warnings` pass in an isolated worktree. The database/relay tests themselves were not run because they require disposable PostgreSQL/Redis and the built relay; GitHub reports no checks for this branch. No bypass was found in the reviewed relay access paths.
- **Discussion state:** the latest GitHub GraphQL query found no unresolved inline review threads on open RustShare PRs #488–#498. This does not satisfy fresh review approval for changed heads or turn failed/in-progress checks into passes.
- **Qualification decision:** local actionable observer race fixed and committed on #489; no additional locally reproducible code defect found in the reviewed #488–#498 heads or Buzz #7 pass. Code/security review completion remains unestablished while exact-head CI and fresh human review are outstanding. Operational qualification remains incomplete; pilot remains **NO-GO**.
