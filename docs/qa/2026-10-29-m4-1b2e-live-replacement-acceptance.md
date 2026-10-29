# M4.1b2e live identity replacement acceptance

Status: partial. Production replacement remains disabled.

The acceptance build adds a debug-only qualification fixture for the pinned
Buzz source revision `2a4a8d926d79b406b7058419ef84c319a0c107fd`. It reads the
checkout SHA from the supplied source directory, restricts the executable to
the supplied build target directory, and hashes that executable at runtime.
Release builds compile the fixture out. The
committed production compatibility manifest still sets
`BUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED=0`, and the production approved-build
list remains empty.

This fixture is not an approved Buzz release and does not enable replacement
in a production build. The pinned Buzz relay was built and its standalone
authoritative-membership live conformance passed against disposable PostgreSQL
and Redis. That upstream proof does not exercise Elembra's authenticated
replacement lifecycle. The integrated live-service harness and blocking CI job
still need to be added before this milestone can pass.

Local evidence so far:

- Buzz source SHA: `2a4a8d926d79b406b7058419ef84c319a0c107fd`.
- Buzz source tree: `a5cc0b56d6235ce800a515e7298aea155118a322`.
- Debug relay SHA-256: `02ad0ceaac00d8c20ef49e98e702227711209d16ff168eb04cd57b0b9ad398a3`.
- Buzz `membership_v1alpha2` migration/receipt/restart acceptance passed
  against disposable PostgreSQL (1 test).
- Acceptance fixture source/digest checks and default-disabled policy checks
  passed.
- Elembra migrations through `20261029100000` applied to disposable
  PostgreSQL; the Chat identity lifecycle regression suite passed (3 tests),
  as did the replacement evidence/transaction rollback test.
- Server all-target/all-feature Clippy and formatting passed. The structural
  no-ACL guard passed.
- OpenAPI export tests passed (3 tests); SQLx workspace metadata check passed
  with the repository's existing unused-query warning.
- No integrated Elembra/Buzz replacement scenario, normal Chat product
  acceptance, or protected CI run has been executed.

## Required evidence still outstanding

- Start isolated Elembra and Buzz PostgreSQL databases, RustFS, Redis, backend,
  dispatcher, replacement worker, and the source-built Buzz relay.
- Drive challenge, NIP-42 proof, guarded initiation/status, conditional NIP-43
  removal and admission, signed membership read-back, and final local
  activation through authenticated Elembra HTTP and the real relay.
- Execute the specified delivery, concurrency, mapping, evidence, tenant,
  audit, and database fault cases, plus normal Chat compatibility checks.
- Add the blocking `Buzz Identity Replacement E2E` workflow job, preserve
  diagnostics, and verify its branch-protection enforcement.
- Obtain independent security review before any merge or production
  qualification.

No integrated Elembra/Buzz acceptance services or replacement E2E cases were
run for this report.
