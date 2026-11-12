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

## Current gate state

- #488 is ready-for-review and approved at `717977b31c62041918dc1599e47488e4de12cbce`; all required checks and exact-head Chat Product Acceptance/integration passed, and the exact-index channel suite is 13/13. The image digest/provenance dispute is resolved. It remains unmerged and undeployed under the explicit release gate; its own PR body also states it is not merge-ready before operator gates.
- #489 monitoring fix passed CI and independent review; `senolcolak` approved exact head `1f169db…` and the addressed bot thread was resolved.
- #490 still documents synthetic local restore mechanics. Cross-stack implementation is in #493. Safety/restore tests and independent code review pass; three technical discussion threads are resolved. Full drill is blocked because `/var/lib/containerd` resolves to `/srv/data01/containerd` on `/dev/sdb`, 100% full. No recovery containers/volumes/networks remain; tags `elembra-g0-recovery-20261112t224629z-21447-chat-observer:latest` (ID `5528b1d6cb48`) and `elembra-g0-recovery-20261112t224629z-21447-chat-bootstrap:latest` (ID `db437104fd74`) remain because Docker cannot update metadata. Do not retry Docker until the host operator frees that failure domain.
- #491 is a triaged security snapshot, not alert closure. `senolcolak` approved exact report head `dcfc71d8a0c3d7efee9d87a830d9b26736770a91`; 60 Dependabot alerts remain open, with conditional or unpatched findings gated for owner disposition. No alert was dismissed.
- #492 is at `ec3c5ed550f8fd7da5222e36570faf4ac2ed71c2`; the observer packaging regression and literal `&nbsp;` corruption are fixed. Focused mail helper tests pass 4/4, exact-head independent review found no further issue, and all exact-head CI including integration passed. Senol approved this exact head. It remains unmerged; a push to `main` triggers the pilot-image publisher workflow, and this phase did not authorize publication.
- #493 is at `af94bf443abde2902688e2ad3fd7c090a68d6f80`. All exact-head CI, including Chat Product Acceptance and integration, passed; independent review found no remaining issue and Senol approved the exact head. All three technical review threads are resolved. The local cross-stack backup/restore drill remains unqualified because `/srv/data01` containerd storage is full. Two recovery image tags remain; do not run Docker cleanup until the host operator resolves capacity. No recovery PASS is claimed.
- No PR was merged in this loop. A merge to `main` triggers `.github/workflows/pilot-release.yml` and its `publish-pilot-image` job, so keeping merges separate avoids publishing a pilot artifact as a side effect of review remediation.
- The real pilot hostname/host, protected-environment access, external backup failure domain and independently held recovery keys, host alert ownership, and real notification destination are external gates.
