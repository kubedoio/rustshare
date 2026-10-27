# M4.1b2 release qualification — Buzz v1alpha2

Audit date: 2026-10-27. Decision: **NO-GO for human merge**.
No merge, deployment, production pin update, community opt-in or replacement
enablement is authorized by this report.

## Exact source and evidence

- Elembra #426 merged as `b8bbe98f72d51fc026ee5e011242490fb518b650`;
  reviewed head `eb66b124898b670b2ea5be535e9a20d3e86bed93`.
- Elembra #427 merged as `3fbfd641690e07a109feb7294a255f5de3ba88cc`;
  reviewed head `3c75bb275e5d07f8ae5b294dae23dafc81329a7e`.
- [Buzz PR #6](https://github.com/kubedoio/buzz/pull/6) original reviewed head
  `7c4e2a0df0ff229a0d8c4b0d9d84fffe91d93b75`, base
  `7fa6bf960d74e6a498caa772b0a81c4142666c9f`; draft, open, unmerged.
  Security fix pushed as **`8ecd46f0bbdfd9042243ecb2076f77fe90556dc6`**;
  followed by registry/test fix **`c3bcde053a5845b5ca16e2f8deb8d9bf2f18fc5a`**;
  the latter is the current qualification candidate and requires fresh human review.

Elembra head `3c75bb2…` reports these GitHub Actions successes:

| Check | Result / evidence |
| --- | --- |
| integration-tests | [success](https://github.com/kubedoio/rustshare/actions/runs/40534575289/job/122296268659); actual `Buzz v1alpha2 migration and live conformance` step succeeded |
| Buzz Live Conformance | [success](https://github.com/kubedoio/rustshare/actions/runs/40534575289/job/122296227293) |
| Chat Product Acceptance | [success](https://github.com/kubedoio/rustshare/actions/runs/40534575289/job/122296227371) |
| Clippy | [success](https://github.com/kubedoio/rustshare/actions/runs/40534575319/job/122296226145) |

These are counterpart evidence, not independent Buzz CI or qualification of a
future immutable release image. Re-run against the accepted release digest.

## Audit coverage and finding

Reviewed the complete original 11-file diff, not just PR prose: workflow,
database lib and relay-members API, migration, schema snapshot, relay read
handler, admin mutation handler, router, PostgreSQL test, live relay test and
contract document. Connected auth/startup/alternate membership paths were also
reviewed. GitHub returned no submitted reviews, no inline review threads and
no top-level review findings on the original head. Therefore there is no human
approval to carry forward. The prior self-reported fixes were checked in source:

| Area | Source result and remaining limit |
| --- | --- |
| Trusted-service NIP-98 | Exact host-derived URL, payload hash, signature, method, allowlist and fail-closed replay gate; live auth/replay checks present |
| Signed response / isolation | Kind 19030, exact UUID/community/public key, primary DB read; no cache/replica/channel substitute |
| Signing identity | **Found medium issue:** membership check could sign with main's public development fallback key when stable identity absent; existing discovery already rejected this. Fix returns HTTP 503 before admission/replay/DB. Production pinned clients reject a mismatched fallback, limiting impact. Live regression exercises the fallback configuration. |
| Transaction consistency | READ COMMITTED required before authorization; community lock serializes actor authority and target revision/receipt; stale repeatable snapshots rejected |
| Revision and receipts | Atomic mutation/revision/receipt; bigint overflow fails; ID binds actor/community/target/kind/role/expected revision; stale rejection durable; no-op revision stable; replay emits no duplicate announcement |
| Sticky revocation | Removal of ABSENT creates tombstone; revoked key cannot be re-added; trigger guards legacy inserts/deletes/role changes; alternate invite/owner/operator paths checked |
| Migration / restart | Active schema-28 rows seeded at revision one, deleted history unrecoverable; same-owner startup supported; migration/schema function bodies match |
| Secrets | No new plaintext private-key logging identified; human-key custody remains outside Elembra |

The added affected-library gate also exposed two source failures: the embedded
migrator test expected 28 rather than 29 migrations, and the tenant schema lint
rejected the intentionally global receipt operation-ID primary key. The fix
updates the migration assertions and explicitly registers receipts alongside
the existing global replay registries. Only UUID uniqueness is global: receipt
ownership, community FK, actor and exact-command authorization remain scoped.
Focused migration assertions preserve every receipt comparison, and PostgreSQL
tests compare unchanged receipts/both communities after conflict and race a
single operation UUID across communities to prove at most one effect/receipt.
This retains cross-community reuse rejection instead of silently weakening the
key or removing the isolation lint. Known-ID collisions intentionally couple
availability; concurrent collisions may abort and roll back, then require retry
with the same ID. The mutation HTTP response does not disclose prior revision.
Fresh independent human review must explicitly accept this global-registry
classification. Migration 29 is still unreleased; its checksum changes, so
retest from fresh schema 28 rather than reusing an already migrated fixture.

Independent agent security review covered all 11 files and connected code and
reported the fallback-key finding. This is useful review evidence, **not an
independent human architecture/security approval**. Fresh human review of the
final exact SHA remains mandatory after the fix. No reviewer identity was
invented and no approval was submitted on the author's behalf.
Fresh agent review of the fix and live regression found the signing-identity
finding resolved in source with no additional finding. That review covered
the working diff subsequently committed as `8ecd46f0bbdfd9042243ecb2076f77fe90556dc6`.
Another fresh agent review covered the registry/test diff committed as
`c3bcde053a5845b5ca16e2f8deb8d9bf2f18fc5a`, with no additional blocking finding.

## CI enforcement and blocker

The workflow has unfiltered `push`, `pull_request`, and `workflow_dispatch`
triggers, read-only contents permission, disposable PostgreSQL/Redis, mandatory
ignored PostgreSQL and actual-relay tests, and a 45-minute timeout. This audit
also adds affected relay Clippy and affected-package library tests.

Original exact Buzz head had zero Actions runs, zero check runs and pending
combined status with zero statuses. Repository Actions permissions reported
`enabled=true`, `allowed_actions=all`; the new workflow was absent from the
registered workflow list. Manual dispatch of its filename returned HTTP 404.
Closing/reopening the draft PR preserved its head but still produced no run.
Dispatching existing `ci.yml` provided a decisive API response:

`HTTP 422: Actions has been disabled for this repository.`

Reapplying `PUT /repos/kubedoio/buzz/actions/permissions` with enabled=true
returned success and enabled=true, but dispatch still returned the same 422.
The underlying organization/account/GitHub restriction is not established.
A repository/organization owner must restore execution; do not assume a
workflow-bootstrap merge fixes repository-level disablement. Do not push an
untested workflow directly to main. If registration still needs bootstrapping
after restoration, use an independently reviewed bootstrap PR while preserving
protection and a valid check on that PR; no exemption for #6.
After pushing `8ecd46f0bbdfd9042243ecb2076f77fe90556dc6`, the Actions/check-runs
APIs still report zero runs and zero checks for that exact SHA. No green Buzz
check result is claimed.

Main protection previously had strict checks but an empty required-check list.
This audit configured the exact required context
`Buzz v1alpha2 PostgreSQL and live conformance`, bound to GitHub Actions app
ID 15368. Read-back confirms strict=true. Existing admin enforcement, one
approval, stale-review dismissal, conversation resolution, no force pushes and
no deletion were retained. PR #6 subsequently reports `mergeable_state=blocked`.

**Failed-check enforcement demonstration is not complete.** Missing-check plus
draft/review blockers are not proof that a failed required check alone blocks
merge. Once Actions is restored, run a disposable probe PR against protected
main with a deliberately failing instance of this workflow context; record its
head, actual GitHub Actions failure, required-check association and blocked
merge state. Make no merge call. Repair the probe, rerun green, and close it.
Never manufacture a success status or use an administrator bypass.

## Local validation and limits

Disposable dependencies were created only for this audit:
`m41-buzz-qualification-postgres` on loopback 55442 and Redis on 56442.
The tests create isolated databases, upgrade real migrations, launch the actual
compiled relay, restart it and briefly pause only that disposable database.
Initial audit run: PostgreSQL contract 1 passed, live relay contract 1 passed.
Fresh retest of the code committed as `8ecd46f0bbdfd9042243ecb2076f77fe90556dc6`:
relay build passed; schema 28→29 PostgreSQL contract **1 passed**; actual relay
conformance **1 passed**, including the unconfigured signing-identity HTTP 503
regression; Rust formatting passed. Affected `buzz-db` and `buzz-test-client`
all-target Clippy with warnings denied passed. Logs:
`/tmp/m41b2-qualification-retest.log` and
`/tmp/m41b2-qualification-db-client-clippy.log` on this runner (ephemeral evidence,
not GitHub CI artifacts). No infrastructure-unavailable skip is counted as success.
Relay binary Clippy with warnings denied also passed on this candidate. Relay
all-target/library validation remains subject to the disk limit below.
After correcting the library failures, `buzz-db --lib` passed 94 tests (154
infrastructure tests ignored by that ordinary unit invocation) and
`buzz-test-client --lib` passed 4. The dedicated ignored PostgreSQL and live
tests are executed separately; ordinary ignored counts are not conformance.
Final retest of code committed as `c3bcde053a5845b5ca16e2f8deb8d9bf2f18fc5a`
passed those 98 unit tests, relay build, PostgreSQL migration/receipt/concurrency
contract (1), live relay conformance including fallback identity (1), db/client
all-target Clippy, relay binary Clippy and formatting. Final runner log:
`/tmp/m41b2-qualification-final-retest.log`. Relay all-target Clippy/library
tests and full `just ci` remain incomplete; narrower successes do not satisfy
those broader gates.

Broader relay library test compilation and affected all-target Clippy were
attempted but exhausted local disk (`No space left on device`); the Clippy
attempt also emitted compiler SIGSEGV under that exhaustion. These checks
are not green evidence. Generated affected package artifacts were cleaned;
no source/user data was deleted. Full `just ci` and protected Buzz checks remain
required. Prior logs and Elembra green checks cannot replace these results.

## Merge acceptance checklist

- Restore GitHub Actions and obtain all required Buzz checks green on the exact
  current head, recording source SHA and any tested merge SHA separately.
- Complete the failed-required-check enforcement proof without a merge attempt.
- Obtain independent human security/architecture approval on the final head;
  re-request after substantive changes.
- Resolve all actual review threads and confirm current base/head mergeability.
- Verify v1alpha1 compatibility and Elembra acceptance for the release candidate.

Until every condition holds, decision remains NO-GO. Once satisfied, report GO
for **human merge only**. Never auto-merge.

## Immutable release and separate pin-update plan

Only after a human merges #6, capture the actual merge SHA from GitHub; no Buzz
merge SHA or release digest exists for this audit. Build from that immutable
source, recording Dockerfile/build context, pinned builder/base image digests,
Cargo.lock, Rust toolchain, target/architecture, build args and source-date
inputs. Audit the existing Docker/release workflow before reuse. Produce an
SBOM and signed provenance linking repository, merge SHA, builder identity and
workflow run to the pushed registry digest. Verify signature/attestation subject,
trusted workflow identity and pulled digest. Rebuild independently and compare
digests; if builds differ, identify nondeterministic inputs rather than claiming
bit reproducibility. Never qualify a floating tag.

On isolated production-like PostgreSQL, take and restore-test a pre-upgrade
backup, record schema 28 and representative active/deleted history, migrate
28→29 with that exact image, and verify rows, revisions, isolation and restart.
Run existing v1alpha1 compatibility, v1alpha2 live conformance and Elembra Chat
Product Acceptance against the **same image digest**. Exercise DB/relay outage
and backup recovery with explicit test data. Record commands, versions, results,
image/platform digests and provenance in durable CI artifacts.

Rollback is application rollback retaining schema 29, guards, receipts and
tombstones. There is no supplied destructive down migration. An older binary
cannot safely write an opted-in community; keep it isolated/read-only unless
compatibility has been demonstrated. Restoring a pre-upgrade backup loses later
writes/tombstones/receipts; it is disaster recovery with an explicit data-loss
window, not a non-destructive rollback. Never drop guards or clear opt-in to
make legacy writers work. Test a compatible prior binary with schema retained
and demonstrate rejected legacy writes before calling rollback safe.

Submit a **separate** compatibility-pin update PR with exact source SHA,
digest/provenance, test evidence, backup/rollback limits and human review. Do
not alter production pin in the Buzz source PR. Existing communities remain
v1alpha1 until all legacy workers/writers are upgraded, drained or safely
quarantined, queued admission events accounted for and opt-in explicitly
approved under the membership-table lock. Historical deleted keys read ABSENT;
Elembra's historical-key prohibition must remain.

Next: [M4.1b2c specification](../specs/chat-durable-identity-replacement-m41b2c.md)
and [implementation prompt](../plans/m41b2c-next-implementation-prompt.md).
