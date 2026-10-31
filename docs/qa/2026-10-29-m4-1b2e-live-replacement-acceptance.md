# M4.1b2e live identity replacement acceptance

Status: **PARTIAL — NO-GO for merge or production qualification.** The complete
happy path executes, but the acceptance suite finds a real authorization gap.
PR #433 supplied only the debug qualification fixture; this work reuses it
unchanged. Production replacement and production v1alpha2 opt-in remain disabled.

## Reproduce

From a clean checkout with Docker, Rust 1.97.1, Node 22 and npm:

```bash
./scripts/run-buzz-replacement-e2e.sh
# Include PostgreSQL regressions, OpenAPI, SQLx and upstream migration/live checks:
REPLACEMENT_E2E_VALIDATE=1 ./scripts/run-buzz-replacement-e2e.sh
```

The script builds Buzz from `2a4a8d926d79b406b7058419ef84c319a0c107fd`, checks
its clean checkout, computes this build's executable SHA-256, and supplies
#433's four fixture variables. Elembra uses the debug-only
`test-buzz-replacement-qualification` feature. The executable is **not** an
approved production release. No hash from another build is used for qualification.

The driver starts two PostgreSQL containers, Redis, RustFS, Buzz with schema 29,
and the real Elembra binary with its outbox dispatcher and replacement worker.
Images use digests; ports bind to loopback. Each run owns separate containers,
credentials and keys. Explicit process environments and an empty process-directory
`.env` prevent dotenv from loading parent deployment files. Human signing keys
stay in the driver; Elembra receives signed NIP-42 proofs. SQL sets up disposable
fixtures, reads state or injects named faults; successful membership mutations
always use the real relay protocol.

Evidence defaults to ignored `.agents/evidence/replacement.*` directories.
`results.json` contains source/build identity, outcomes, durable command IDs,
signed-observation IDs and audit transitions. Logs are redacted and bounded.
Cleanup removes only run-owned resources. The `Buzz Identity Replacement E2E`
CI job uploads JSON/log files, never `.env`, keys or source/target directories.
Failures fail the job; no acceptance assertion is ignored or optional.

## Executed lifecycle and faults

Retained local `.agents/evidence/run2/results.json` has 19 passed checks and
one failed safety check. Buzz tree:
`a5cc0b56d6235ce800a515e7298aea155118a322`; computed executable SHA-256:
`02ad0ceaac00d8c20ef49e98e702227711209d16ff168eb04cd57b0b9ad398a3`.

Operation `da97103a-1523-4204-a8f2-6c72e76aa0dc` completed after relay outage
and backend restart. HTTP initiation revoked the old local binding/admission
and created a pending candidate. The worker delivered conditional NIP-43 removal
`a6682ed2-494b-4f3a-a211-44941991b619` at expected revision 1, accepted fresh
signed old-key REVOKED/candidate ABSENT evidence, delivered admission
`14d5acdf-8b4e-4625-8243-3dd3eff2e820` at revision 0, accepted signed ACTIVE
evidence, then atomically activated the binding and recorded tenant-scoped
completion. The old key could not send, reconnect or regain membership; the
candidate could send after completion. Historical attribution/signature survived.

Additional executed checks: foreign tenant/wrong Principal access, wrong
workspace/old binding/candidate proof, expired challenge, exact initiation replay,
audit transaction rollback, permanently revoked candidate with audited failure,
lost ACK with identical signed-event/command UUID retry, duplicate command UUID,
invalid signature and replayed signed observation. The rerun additionally covers
v1alpha1 onboarding/message/revocation/offboarding before test-only v1alpha2 opt-in.

## Blocking security finding

For operation `e9475a4e-a7a0-402c-a27e-c48785796426`, the proxy forwarded a real
successful admission and withheld the real final signed read. Elembra correctly
kept binding `pending` and operation `pending_membership_verification`; relay OK
alone did not complete replacement. Nevertheless, a candidate-signed message to
the same authorized open channel received this real Buzz response:

```json
["OK", "8fa821e93f921e1f3cb6a2fdd5233404628b32bda59be12a5f7aa0ecedebf122", true, ""]
```

The required no-sending-before-verified-completion assertion fails. No forged
successful mutation or direct membership SQL update produced the result. Buzz
authorizes active membership without knowing whether Elembra completed its final
verification/commit. UI gating cannot enforce this boundary for a client holding
its signing key. An independently reviewed protocol/contract resolution is
required. This PR keeps the failing assertion, adds no second replacement
architecture, and does not change the accepted Buzz pin.

## Connected review and separate fixes

- `e5ef2c90`: conditional commands previously sent EVENT before authentication,
  causing v1alpha2 to close the connection. They now wait for the matching
  successful NIP-42 ACK. A unit test covers ACK acceptance/rejection and proves
  EVENT is withheld until authentication. Legacy first-send behavior is retained.
- `87b53fb8`: Chat's manifest omitted admission/revocation ownership, so the
  outbox rejected worker commands. It now declares exactly those two existing
  contracts, with an unrelated-event rejection assertion.

Security note: qualification, tenant fences, key custody, signed evidence,
CAS/leases and production enablement are unchanged. Both fixes require human
security review.

## Validation and outstanding gates

- Workspace libraries: 1,334 passed, zero failed, 70 existing ignored tests.
- Server/core all-target/all-feature Clippy: passed.
- Earlier v1alpha1 conformance: 13 passed, but temporary logs were lost during
  an execution-environment restart; this is not retained artifact evidence.
- PostgreSQL regressions, SQLx, OpenAPI, release fixture exclusion, existing live
  conformance and Chat Product Acceptance results are recorded separately as
  they finish; component tests are never treated as integrated acceptance.
- Production policy remains `BUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED=0` with an empty
  approved-build list. The test fixture remains unchanged from #433.
- Main protection does **not** require the new job. Only request enforcement
  after it passes, then demonstrate a failed required check on a safe unmerged
  probe without calling merge or bypassing protection.
- Independent human security review is outstanding; M4.1b2f must wait.

The complete requested fault matrix is not implemented: worker crashes on both
sides of ACK persistence, lease expiry, stale revision/reordering, concurrent
starts, Principal disable during replacement, lock-wait challenge expiry,
generation/CAS/mapping rotation, unexpectedly active old key, separate DB commit
failure and legacy outbox transition protection need dedicated integrated cases.
Even adding those cases cannot close the milestone while pre-completion sending
still succeeds.

## Final retained execution record — 2026-10-30

This section supersedes the earlier pending validation notes above. The final
local acceptance run used Elembra `e28aac1c56a33d9a7ec6afd1a63a09b11c3e8672`,
Buzz source `2a4a8d926d79b406b7058419ef84c319a0c107fd`, Buzz schema 29, and the
freshly built debug executable SHA-256
`02ad0ceaac00d8c20ef49e98e702227711209d16ff168eb04cd57b0b9ad398a3`. Its
reproducible command is `scripts/run-buzz-replacement-e2e.sh` (optional extended
checks used `REPLACEMENT_E2E_VALIDATE=1`). The ignored local evidence is under
`.agents/evidence/run4/`; acceptance summary is `results.json`.

- **Happy path:** passed end-to-end through authenticated HTTP, NIP-42, durable
  dispatcher/worker, conditional NIP-43 removal/admission, signed REVOKED,
  ABSENT and ACTIVE observations, local atomic completion, audit history,
  restart recovery and historical message attribution.
- **Acceptance cases:** 20 passed, 1 intentionally failing safety assertion.
  The full request matrix remains incomplete, as enumerated above.
- **Blocking assertion:** with the final signed membership response withheld,
  operation `cc2524a5-d0d4-4258-b183-44d77fd0231f` was still
  `pending_membership_verification` and the local candidate binding was pending;
  Buzz accepted candidate event
  `560c7fa1084949d28761856a719c7a2178c39f83a2d99939a23619ae16860603`. This
  proves Buzz admission currently precedes Elembra's final
  verification/activation boundary. The assertion remains enabled and failing;
  relay acceptance is not treated as Elembra completion.
- **Extended checks:** PostgreSQL regressions 3/3, OpenAPI tests 3/3, Buzz
  migration-29 test 1/1, live authoritative membership test 1/1, and existing
  Buzz live conformance 13/13 passed. SQLx metadata check passed (one existing
  unused-query warning). Rust workspace libraries: 1,334 passed, 0 failed,
  70 existing ignored; affected server/core all-target/all-feature Clippy,
  formatting, release fixture exclusion, handler SQL guard (591 sites and 7
  guard tests), no-ACL structural guard, frontend check/lint/build passed.
- **Other validation and follow-up:** the initial frontend run had 4 date-sensitive
  Calendar failures and the dependency audit had one high `happy-dom` finding.
  The fixtures now stay within the visible month and the test dependency was
  updated to `happy-dom` 21.0.4 without forcing other dependency upgrades.
  Follow-up frontend results: Calendar tests 50/50 and full frontend tests
  1,352/1,352 passed; `npm audit --audit-level=high` exits successfully with
  eight low findings remaining. Frontend check/lint/build passed. The local
  Chat Product Acceptance harness could not start because an unrelated
  pre-existing service owned port 80; only this task's isolated compose project
  was stopped. Its CI job was still in progress at the last check.
- **Dedicated CI:** `Buzz Identity Replacement E2E` ran in workflow
  [41045465713](https://github.com/kubedoio/rustshare/actions/runs/41045465713),
  job 124028357826, against PR head `1cafe2882083a1a0c9c14e83cad52e808458e0de`
  (workflow merge SHA `c31c87b4e45e32d6bfabcd059b6120357ec6098e`). It failed at
  the same candidate-send safety assertion. CI's build-specific Buzz executable
  SHA-256 was
  `48f5cc22c32c3f983d6d045f7c96437a9991ffe5f11944d79b6365fb79ba6d43`.
  Diagnostics upload and owned-container cleanup passed. This hash is evidence
  for that CI build only, not a production qualification tuple.
- **Policy and review:** production qualification remains disabled and the
  approved-build list empty. Main branch protection does not include the new
  check. No required-check probe was attempted because acceptance is red.
  Draft PR #434 requests review only from `senolcolak`; independent security
  review is pending. No reviewer outside the user's allowed list was requested.

The PR's two CodeQL comments identified raw relay WebSocket response data in
the persisted pre-activation probe. The JSON evidence now stores only the
boolean relay-accepted result; raw network response content is not written to
the evidence file. CodeQL re-analysis is pending on the follow-up commit.

**Disposition: PARTIAL / merge NO-GO / production release NO-GO.** Resolve the
pre-completion Buzz-send contract, complete the remaining fault matrix, obtain
independent security review, and get green protected CI before closing M4.1b2e.
M4.1b2f remains deferred.

## Follow-up execution — 2026-10-30

The staged-send defect above is fixed by the upstream Buzz staged-membership
release change and the Elembra bridge now requires the signed gate-open
observation before it completes the release. The following run used the
committed Elembra tree `fe1dbe202dd0532be19ff9b3884c79a69bb4d5fa` and Buzz source
`e22ba1ed44822fd47a6c31362eea5087a8730d74` (tree
`163d0baf005b449838febe0b5667fb48733adf66`), schema 30. The actual debug relay
SHA-256 was
`2b5949ef0a856028fe6d76d9b626bca68937358d29e5d1fe395e605681f439f8`.
Command: `REPLACEMENT_E2E_VALIDATE=1 scripts/run-buzz-replacement-e2e.sh` with
the #433 test fixture variables pointing to that Buzz source and target.
Disposable-run evidence is `/tmp/m41b2e-integrated-committed/results.json` and
the driver log is `/tmp/m41b2e-integrated-committed-driver.log`.

- **Live acceptance:** 22/22 passed. This included the real HTTP/NIP-42
  replacement flow, conditional NIP-43 revoke and staged admission, signed
  REVOKED/ABSENT/ACTIVE observations, local completion and audit, followed by
  the signed Buzz release. The old key remained revoked and blocked; candidate
  sending stayed blocked until the verified release. Historical attribution,
  v1alpha1 compatibility, normal message delivery, reconnect denial and
  offboarding also passed.
- **Executed recovery/security cases:** relay outage and durable retry; lost ACK
  with stable command UUID; duplicate/replayed UUID; wrong tenant, Principal,
  workspace, binding and candidate proof; expired challenge; audit rollback;
  permanently revoked candidate; invalid/replayed signed evidence; and relay OK
  without verified completion. PostgreSQL regressions passed 3/3, OpenAPI tests
  3/3, SQLx metadata passed, Buzz migration-30 test passed 1/1, and the
  authoritative live Buzz membership test passed 1/1. Handler SQL guard passed
  (591 reviewed sites), no-ACL guard passed, affected all-target/all-feature
  Clippy and formatting passed, workspace library tests passed (1,335 passed,
  70 existing ignored), and a release build with the fixture feature passed.
- **Still outstanding:** this is not the entire requested injected fault matrix.
  Dedicated cases for lease expiry, concurrent starts, Principal disable during
  an operation, mapping rotation, definitive stale-revision recovery, command
  reordering, unexpected old-key reactivation, failed DB commit, and operator
  visibility for orphaned pending commands are still absent. CI results below
  record which connected acceptance jobs passed and which integration sweep
  exposed the stale fixture.
- **CI/review:** PR run
  [41083173638](https://github.com/kubedoio/rustshare/actions/runs/41083173638)
  passed `Buzz Identity Replacement E2E` (job `124158142968`, 22/22), `Buzz
  Live Conformance`, and `Chat Product Acceptance`. Its accepted Buzz build
  SHA-256 was
  `372f1b1eb551afc4d73746fabcca09de3a1fd0d800829d61b4da67fa827a5674`.
  The same run found the broad ignored-test sweep had a stale mock in
  `membership_http_persists_single_use_nonce_and_rejects_replay`: it omitted
  the new signed `send_enabled` and `staged_operation_id` fields. The mock now
  returns the open-gate values, and that exact test passed 1/1 against a fresh
  pgvector-backed disposable PostgreSQL with all migrations. The integration
  workflow must be rerun on the follow-up commit.
- **Protection/review:** `main` now requires the exact `Buzz Identity
  Replacement E2E` status from GitHub Actions (app 15368), verified via the
  branch-protection API. The safe unmerged failed-check probe remains pending.
  PR #434 is draft; old review threads are resolved. Senol (`senolcolak`) is
  the only requested reviewer because Zoorpha authored the PR. Independent
  human security review is still pending.
- **Upstream CI:** Buzz PR #7 remains draft at the exact source SHA above and
  has no reported checks. A manual dispatch attempt was rejected by GitHub
  with “Actions has been disabled for this repository”; the immutable Buzz
  source was instead built and tested by the RustShare E2E job and local Buzz
  DB/relay Clippy and migration/conformance commands. Do not treat that as an
  upstream PR CI pass.
- **Production:** compatibility qualification remains disabled, the production
  Buzz pin and allowlist are unchanged, and the #433 fixture is still debug
  only. This source-built runtime is test evidence, not a production release.

**Checkpoint disposition before final protected acceptance: PARTIAL / merge
NO-GO / production release NO-GO.** At that checkpoint, the specific
pre-completion send defect was resolved and the integrated lifecycle passed
locally. The final CI and required-check probe results are recorded below;
remaining fault coverage and independent human security review still gate
closure. M4.1b2f remains deferred.

## Protected acceptance at f7ab checkpoint — 2026-10-30

On PR head `f7ab026b109ac7d12f2500b643a1add3dca71f3d`, GitHub workflow
[41088100029](https://github.com/kubedoio/rustshare/actions/runs/41088100029)
passed all required checks:

- `Buzz Identity Replacement E2E` job `124174500687`: 22/22 acceptance checks
  passed. The retained artifact is `buzz-identity-replacement-e2e`; its Buzz
  source was `e22ba1ed44822fd47a6c31362eea5087a8730d74`, schema 30, and actual
  executable SHA-256
  `372f1b1eb551afc4d73746fabcca09de3a1fd0d800829d61b4da67fa827a5674`.
- `Buzz Live Conformance`, `Chat Product Acceptance`, and `integration-tests`
  passed. The integration job ran 1,151 tests with zero failures; its broad
  ignored-test sweep now includes the repaired membership mock and the two
  repeated Ask Workspace security matrix executions.
- Rust Clippy, Format, Cargo Deny, frontend, DCO, CodeQL, OpenAPI, plugin tests,
  compose contracts, and terminology scan passed. SQLx Prepare Check, general
  unit-test job, Build Release, and Code Coverage were skipped by the workflow's
  conditions. Equivalent SQLx metadata, workspace libraries, formatting,
  fixture-enabled release compile, and relevant Clippy checks passed locally.

Main protection now requires `Buzz Identity Replacement E2E`. The safe
failure-blocking probe used temporary PR #435 with one intentional failure in
that exact job. GitHub reported the required job `fail` and PR `BLOCKED`; the
probe PR was closed without merging and its branch deleted. Main's existing
required contexts were preserved.

Buzz PR #7 remains a draft and has no reported upstream checks. GitHub rejected
manual workflow dispatch with “Actions has been disabled for this repository.”
The RustShare acceptance job nevertheless built and exercised the exact
immutable Buzz source. Do not treat this as a Buzz PR CI pass.

**Disposition at the f7ab checkpoint: PARTIAL / merge NO-GO / production
release NO-GO.** The integrated happy path, currently implemented fault cases,
normal Chat checks, protected CI, and required-check failure enforcement are verified. The
remaining requested fault cases listed above and independent human security
review are still release gates. PR #434 remains draft; only Senol was requested
because Zoorpha authored it. Production qualification remains disabled.

## Upgrade review finding — 2026-10-30

Codex identified that immutable durable-consumer subscriptions would reject the
new release event on existing installations. Migration
`20261030120000_buzz_bridge_release_subscription` extends the registered
`io.elembra.chat.buzz-bridge.v1` contract in place, preserving its identity and
existing delivery ledger; fresh installations still register the full contract
at startup. The ignored database regression test reproduced the prior
registration, applied the migration, and verified the bridge's current
registration succeeds. It passed against a disposable PostgreSQL 16/pgvector
database after all migrations; scoped storage Clippy and formatting passed.
At the time this review note was written, CI for the fix had just started. The
later current-head run is recorded below.

## Final PR review cycle — 2026-10-30

On PR head `c859c8b36009cbdd97561a98251acac452e6c07b`, GitHub workflow
[41095597665](https://github.com/kubedoio/rustshare/actions/runs/41095597665)
passed `Buzz Identity Replacement E2E` (job `124200831066`, 22/22 checks),
Buzz Live Conformance, Chat Product Acceptance, and integration-tests. The
E2E artifact records Buzz source
`e22ba1ed44822fd47a6c31362eea5087a8730d74`, schema 30, and executable
SHA-256 `372f1b1eb551afc4d73746fabcca09de3a1fd0d800829d61b4da67fa827a5674`.
CodeQL, Clippy, Format, OpenAPI, frontend, Cargo Deny, and DCO also passed.
SQLx Prepare Check, generic Test, Build Release, and Coverage were skipped by
workflow conditions.

The durable-subscription review thread is resolved; the disposable migration
regression passed 1/1, with scoped storage Clippy and formatting passing.
Senol approved this head and no review threads remain unresolved. M4.1b2e is
still **PARTIAL / merge NO-GO / production release NO-GO** because the
remaining requested fault/recovery cases have not been implemented. The PR
remains draft and production qualification remains disabled.

## M4.1b2e fault-closure draft — 2026-10-31

The feature branch `feat/m41b2e-replacement-fault-closure` starts from
Elembra main `aea1cc8baec281a3d5cc1453468373c06021ed33` (tree
`3fc0919e489e34196f686b01a5b5c541a7532f7a`). It extends the existing
`scripts/run-buzz-replacement-e2e.sh` /
`frontend/scripts/buzz-replacement-e2e.mjs` harness and retains the required
`Buzz Identity Replacement E2E` job. The test-only Buzz pin is updated to the
then-current PR #7 head `dc09fc2602a7cc64ea15caa865bd773d8be61eed` (tree
`8feb9d59c14b53999dfa16148b2cef62ba28de30`); no production pin or allowlist
was changed.

The harness contains these 12 fault-closure groups:

| Fault group | Status |
| --- | --- |
| Worker crash before relay ACK and after relay ACK | Implemented; not executed |
| Lease expiry and fenced claim recovery | Implemented; not executed |
| Identical concurrent initiation and competing starts | Implemented; not executed |
| Principal disabled during staged verification | Implemented; not executed |
| Mapping and relay-pin rotation mid-operation | Implemented; not executed |
| Definitive stale revision and fresh conditional retry | Implemented; not executed |
| Reordered conditional revocation delivery | Implemented; not executed |
| Unexpected old-key reactivation and tombstone retention | Implemented; not executed |
| Database failure during completion commit | Implemented; not executed |
| Blocked operation recovery by an audited administrator | Implemented; not executed |
| Staged release racing with revocation | Implemented; not executed |
| Lost release ACK and exact-event replay | Implemented; not executed |

Existing acceptance checks remain in the harness and have not been removed:
the v1alpha1 path, relay OK without local completion, send and subscription
gating, cross-tenant access, and ambiguous revoke retry. The historical 22/22
result is preserved as prior evidence only; it is not a run of these additions.

The required invocation was attempted with test-only source and target
directories. Buzz `buzz-relay` built from `dc09fc2602a7cc64ea15caa865bd773d8be61eed`
with SHA-256
`2b5949ef0a856028fe6d76d9b626bca68937358d29e5d1fe395e605681f439f8`.
Elembra `rustshare-server` built from the branch's unchanged backend source
with SHA-256
`1e597e7353cd3b7a9b67d53e900840d5e2ed14c45d572952e35429ec505fd61d`.
`node --check frontend/scripts/buzz-replacement-e2e.mjs` and `git diff --check`
passed; `bash -n scripts/run-buzz-replacement-e2e.sh` and frontend `npm run lint`
passed (0 errors; 155 existing warnings). Rust workspace formatting, Clippy,
SQLx metadata, OpenAPI, structural guards, full PostgreSQL
tests, migration tests, actual relay conformance, multi-node/media tests, and
the frontend acceptance validation were not run. The runner stopped before service startup because the sandbox denied
access to `/var/run/docker.sock`; no new real relay case, migration check,
PostgreSQL test, live conformance test, or validation tail was executed. No
E2E `results.json` or live-run digest pair exists for this draft.

**Disposition: IMPLEMENTED / NOT QUALIFIED / merge NO-GO / production
qualification NO-GO.** The historical 22/22 pass above belongs to the earlier
Elembra head and Buzz `e22ba1ed44822fd47a6c31362eea5087a8730d74`; it does not
prove this test-only source update or any new fault case. Protected CI,
independent security review, and real disposable-service execution remain
required.

## Upstream-only design compatibility and CI follow-up — 2026-10-31

PR #438 protects existing downstream Buzz deployments. The proposed ADR-0044
in PR #439 introduces a separate upstream-only managed identity design;
this acceptance harness does not qualify that design or stock `block/buzz`.
Its signed membership proofs, conditional revisions, tombstones and staged
release remain downstream-specific. Production pins, runtime qualification
allowlists and replacement enablement must remain unchanged.

CI run `41145967534`, testing PR head
`40016029ddefaa83d597bd46e6d1f228214b7b35`, supersedes the local execution
limitation above for that head: disposable services started and the first
four acceptance groups passed. Replacement initiation then returned HTTP 404,
`Chat identity replacement is disabled`, at harness line 966. The independent
integration job failed in the downstream `authoritative_membership` test at
line 521 with `WebSocket(Protocol(SendAfterClosing))`. These are failing live
results, not qualification evidence; the earlier local Docker failure does
not describe this CI execution.

Next: diagnose fixture qualification without bypassing production guards,
resolve the downstream membership test failure, and rerun all required
checks on the resulting exact head. Keep this PR draft and merge NO-GO until
passing evidence and independent human review exist. Run ADR-0044 Phase 1
against its independently pinned unmodified upstream sources in a separate
harness; no result here enables managed key custody or identity migration.
