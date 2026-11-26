# Candidate c2157181: disposable Chat recovery and organization authorization evidence

Date: 2026-11-25. All application data was synthetic. No FWS host, institutional
data, real user, production identity, or published image was used.

## Candidate identity

- Protected-main candidate: `c2157181cad37ed130c02b90e0fb5f08d0970b84`.
- Disposable recovery project: `elembra-g0-recovery-20261125t142024z-29890`.
- Backend image ID: `sha256:bee7554eec0ce44259e7947a7cd06b5e5d69cde70a60e728e32b9184109cb80f`.
- Observer image ID: `sha256:2217ef2192adfe09299269c7cafe2c3f40048daf632da48bcef7289cef2fc263`.
- Buzz relay candidate: `ghcr.io/kubedoio/buzz-elembra@sha256:38c362bb37d15ea879634b0f56a867401df4e4e0074a84382dff6b07bb578b4e`.
- Buzz relay rollback: `ghcr.io/kubedoio/buzz-elembra@sha256:2b6c4b2a1800984283f1893a407997b2d5101b01fd0195669ee5deb84814906a`.

## Recovery and browser journey

Commands from the candidate worktree:

```sh
scripts/test-chat-recovery-drill-safety.sh
scripts/run-chat-recovery-drill.sh
```

The driver created a unique disposable stack, ran `scripts/run-chat-channel-acceptance.sh`
and Playwright `frontend/tests/chat-conversation-acceptance.e2e.ts`, made a local
disposable backup, restored it, checked authorization/projections, replayed observer
events, and switched to the prior relay image to verify rollback.

- Safety/cleanup guard tests: PASS.
- Channel browser matrix: 13/13 PASS.
- Conversation browser matrix: 9/9 PASS.
- Restored Files API: owner byte-hash match; unauthorized request returned 404.
- Restored Chat API projection: channel, event, author, thread and attachment hash
  matched.
- Private-channel history and relay read/write authorization remained enforced
  after restore; removed member could not access or publish.
- Observer recovery: 27/27 replayed rows; catch-up completed.
- Relay rollback: prior immutable image restored and history remained available.
- API projection SHA-256: `f7b228c2dc38e206420cc6459165bea29c811d0f8eafa489e12ad5cd8baec0c4`.
- Event history SHA-256: `85c827e1c0cb6595f64c88138071e2567cf97f615d9f3edaee8a07f3b4aa124e`.
- Files attachment SHA-256: `9fc50d7ca89c6bacae50600fb1712875b9f073b2d5e155d33e0af538a036b6a2`.
- Observer projection SHA-256: `5fce89544e39ec38174061890ff580ee0a0a50585be2d410372e680f7daed1bc`.

The stack, volumes, network and nested worktree were removed after the run. Sanitized
results remain outside the repository at
`/tmp/elembra-g0-recovery-evidence-20261125t142024z-29890.i7jY3N/`. `result.env`
SHA-256: `2900819dfcd1ee34a63c58fdc92d212dda98b26c28c40fd197647f2e1eb9cb8c`.
Raw browser JSON is not copied into the repository.

## Organization group/admin tests on protected main

A fresh disposable PostgreSQL database and loopback RustFS bucket were created and
migrated, used only for these tests, then removed with their Compose project,
volumes, network and temporary environment file. Exact commands:

```sh
SQLX_OFFLINE=true cargo run -p rustshare-server --bin rustshare-server -- --migrate
SQLX_OFFLINE=true cargo test -p rustshare-server --test organization_administrator_grants_test organization_administrator_grants_are_scoped_audited_and_recoverable -- --ignored --exact --test-threads=1
SQLX_OFFLINE=true cargo test -p rustshare-server --test organization_administrator_grants_test organization_admin_http_scope_and_immediate_revocation -- --ignored --exact --test-threads=1
```

Both ignored tests passed (1/1 each) at the exact candidate SHA above. They exercise
scoped administrator grants, audit/recovery invariants, tenant HTTP scope and
immediate authorization revocation.

The related offboarding integration test was attempted but did not execute:
`organization_admin_offboarding_is_transactional_tenant_scoped_and_retry_safe`
failed while linking its test binary (`ld terminated with signal 7 [Bus error]`).
This is a local build limitation, not a test failure or pass. The disposable project
was subsequently cleaned; no offboarding result is claimed.

## Relationship to PR #560 CI

The two local integration results ran at protected-main candidate
`c2157181cad37ed130c02b90e0fb5f08d0970b84`, before PR #560's changes. They are
baseline evidence, not tests of the PR head. PR #560 head is
`a5247718bb42b6343d79f6ebb076c6a255891617`; its Actions runs reported success for
CI, Frontend CI, DCO Check, Vault Sync Compliance, CodeQL, and Integration Tests
(run 1876). Local main-commit results are not credited as PR-head CI or independent
review.

## Unresolved recovery gates

- Off-host backup retention and restore: NOT TESTED.
- Recovery with externally held browser acceptance keys: NOT TESTED; drill keys
  were ephemeral and not exported.
- External recovery credential custody: NOT TESTED.
- FWS DNS, HTTPS, authentication, secret custody, backup destination, monitoring,
  and authorized-user gates: NOT TESTED; operator-dependent.

This evidence qualifies only the listed disposable journeys. It does not qualify a
release, production installer, FWS deployment, or institutional pilot.
