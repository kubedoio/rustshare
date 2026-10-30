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
