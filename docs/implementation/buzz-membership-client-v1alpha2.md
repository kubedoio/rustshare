# Verified membership read client

This bounded client extends `BuzzGatewayClient` using its existing service
credentials, URL validation, redirect policy, body limits, NIP-98 signing
and pinned kind-19030 verification. It introduces no public HTTP route or
binding activation consumer. All v1alpha1 methods remain unchanged.
`services::chat_membership::verify_buzz_membership` owns Elembra receipt
persistence; the gateway remains an HTTP-only adapter with no database pool.

Each call durably records a fresh UUID before sending its authenticated read.
Only the exact community/key/nonce echoes, a valid pinned relay signature,
both event/content timestamps within 30 seconds past and 5 seconds future,
and a revision at least the caller's durable high-water mark are accepted.
Unknown membership, invalid revisions, transport/HTTP failures, stale replies,
wrong pins and persistence failures return an error; callers must deny.
The revision range is 0..=i64::MAX, matching Buzz's non-wrapping BIGINT.
Absent requires revision zero; active/revoked require a positive revision.
Acceptance serializes by relay/community/key and also rejects a revision
below any previously accepted durable receipt, including concurrent reads.
It explicitly uses READ COMMITTED isolation, rechecks both signed timestamps
after waiting for the lock, and uses the database wall clock for request
expiry. An old transaction snapshot or a long lock wait cannot accept an
older revision or an expired response.
The acceptance UPDATE consumes the durable receipt once. Calls after a
restart always generate a new nonce; pending receipts are never resumed.

Migration `20261027130000` stores verification receipts only, never a second
membership authority. There is no database access to Buzz internals. Do not
use the receipt table as an authorization cache. The explicit revision floor
must come from the operation being verified; a zero floor is only appropriate
before that key has an accepted observation. Rollback drops these receipts
only while no durable replacement operation depends on them.

The next milestone must integrate acceptance and the replacement-operation
transition in one Elembra transaction, rather than call this read client and
activate a binding independently. Preserve the request ID and revision floor
with that operation; on crash before committed acceptance issue a fresh read.
The current PR creates no replacement operation, and intentionally leaves the
PR #426 HTTP/storage guards, historical-key prohibition and ordinary first
onboarding paths intact. It enables no pairing or replacement UX.

Independent architecture/security review, PostgreSQL verification tests,
live Buzz conformance, existing Chat acceptance, protected CI and human
approval are release requirements. Do not merge automatically or update
the production image pin in this PR.
