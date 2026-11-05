# Phase 2B.3 publisher contract fixtures

Status: **contract inventory only; all publisher behavior is NOT IMPLEMENTED**.
This test-only slice follows project-owner-selected Option A from
[ADR-0046](../adr/0046-managed-nip29-publishing-authorization.md). It adds no
signer, publishing path, route, or runtime behavior and is not acceptance
evidence for a publisher.

The deterministic fixture uses a fixed authenticated principal, tenant,
workspace, channel, typed `ChannelMessage` intent, empty attachment list, and
canonical UTF-8 request bytes. Its SHA-256 digest is only a fixture checksum;
the future implementation must define and test the production canonical
encoding and atomic idempotency scope.

## Contract inventory

| Future contract | Status |
| --- | --- |
| Current authenticated Principal, tenant, workspace membership, and Chat entitlement | NOT IMPLEMENTED |
| Ready managed identity, key generation, and current relay mapping generation | NOT IMPLEMENTED |
| Typed NIP-29 kind-9 intent with trusted channel binding | NOT IMPLEMENTED |
| Guest, read-only, revoked, wrong-channel, and cross-tenant denial | NOT IMPLEMENTED |
| Atomic tenant/workspace/principal scoped idempotency and canonical request digests | NOT IMPLEMENTED |
| Encrypted immutable event preparation and exact signed-byte retention | NOT IMPLEMENTED |
| Crash recovery before signing, after signing before durable commit, and after commit before send | NOT IMPLEMENTED |
| Lost or ambiguous ACK recovery using the exact event ID and exact signed bytes | NOT IMPLEMENTED |
| Timestamp expiry, stale authorization, identity/key generation, and mapping generation denial | NOT IMPLEMENTED |
| Relay accepted, history observed, and recipient delivered are distinct durable states | NOT IMPLEMENTED |
| PostgreSQL/KMS failure closure and secret/message-body/audit redaction | NOT IMPLEMENTED |

The future test surface for the adapter must exercise duplicate and
conflicting idempotency requests, concurrent claims, authorization changes
between preparation and retry, and exact event-ID reuse. No fixture invents
relay acceptance or delivery. The existing reader does not implement any of
these publisher contracts.

## Gate I matrix — 2026-11-05

| Gate | Result | Remaining condition |
| --- | --- | --- |
| Protected `main` baseline | PASS at `894813735b3a6828149d7216fee4b79ed5d0823d` | Final CI on this commit was still running at review time; report its terminal result separately. |
| #458 managed-history Cilium execution | PASS, run [42056855474](https://github.com/kubedoio/rustshare/actions/runs/42056855474) | Independent issue-level security decision is pending. Execution, ordinary PR approval, and security acceptance are separate. |
| #460 publisher boundary | NO-GO; contracts remain NOT IMPLEMENTED | Upstream-confirmed remediation, official unmodified fixed pin, verified provenance, accepted membership/role regressions, and independent security acceptance are all required. |
| #442 stock Buzz Desktop | UNVERIFIED | No interactive display/session bus is available here. Complete the committed procedure with unmodified stock Buzz Desktop and a working native OS keychain. |
| Gate I / managed Chat activation | NO-GO | Do not activate managed Chat, signing, publishing, or Phase 2B.4. |

Beginning signing or publishing implementation requires the ADR-0046 entry
criteria: the existing owner Option A decision; a named independent security
ACCEPT for authorization, custody, relay trust, idempotency/recovery, replay,
the upstream finding/fix, and redaction; an official unmodified fixed Buzz
revision with upstream confirmation and verified provenance; accepted
membership/role regression evidence; and reviewed typed-request, precheck,
idempotency, exact-byte recovery, encrypted-retention, audit, and executable
test designs.

The issue gates remain separate:

1. #458's managed-history issue-level security acceptance and #442's desktop
   acceptance are resolved under their own policies.
2. The publisher's Rust/PostgreSQL and live disposable Buzz/Cilium contracts
   are implemented and pass before Phase 2B.4.

Until upstream remediation, provenance, regression evidence, independent
security acceptance, and publisher conformance are satisfied, #460 stays
blocked and managed signing stays disabled. #458 and #442 remain independent
Gate I/Phase 2B.4 conditions; they are not upstream remediation evidence.
