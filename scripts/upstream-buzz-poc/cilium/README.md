# Disposable Cilium trusted-service qualification

This harness selects six ignored Rust connector tests against two isolated
instances of **unmodified** upstream Buzz. It creates a fresh local kind
cluster with Cilium, installs default-deny egress in the test namespace, and
allows only the relay dependencies, controlled DNS, and the two TLS edges.
It does not use Docker bridge networking for the Rust connector and does not
touch a host Compose project.

The upstream relay source pin is commit
`eafd77e9f6e96052476bdd88be732c046fdb7ab3`, tree
`04e3a40545bd3b23510ec32b4c8cccee2acef7c5`. The script verifies both before
building the local relay image. Dependency image digests, node image, and tool
versions are recorded in [`versions.env`](versions.env). The relay itself is
not patched or rebuilt from a fork.

The `Managed Chat History Cilium Qualification` Actions workflow runs this
script on a disposable `ubuntu-24.04` runner when its harness or managed
identity transport inputs change. It checksum-verifies the pinned kind,
kubectl, Cilium CLI, and Hubble downloads; uses a unique cluster and image tag
per workflow attempt; uploads only the harness's sanitized evidence after all
tests pass; and removes the cluster, local image tags, temporary credentials,
and local evidence in an unconditional cleanup step. The artifact includes the
exact RustShare and Buzz source revisions, image IDs, tool and image pins,
fixture event metadata, run link, and SHA-256 manifest. A passing run qualifies
only the managed-history and listed supporting transport tests; it does not
qualify publishing, stock desktop interoperability, or Gate I.

## Requirements and command

Use a Linux amd64 host with Docker Buildx, `kind` v0.27.0, `kubectl` v1.32.2,
Cilium CLI v0.16.22, Hubble CLI v0.13.2, Rust 1.97.1, Node.js 22, npm 10+,
OpenSSL, Python 3, and `timeout`. The script fails closed when a required tool
does not match its version.

From a clean RustShare worktree with the `upstream-buzz-runtime` feature:

```sh
scripts/upstream-buzz-poc/cilium/run.sh --preflight-only
scripts/upstream-buzz-poc/cilium/run.sh
```

The first command verifies the pinned source and local tools without creating
a cluster. The second builds the Rust test binary and disposable images,
creates a unique kind cluster, runs the selected ignored tests, and deletes that
cluster and its unique image tags on exit. `BUZZ_SOURCE_DIR` can point to an
already fetched clean checkout at the exact source commit. Set `EVIDENCE_DIR`
to a new empty directory to retain sanitized output at a chosen location; the
default is a mode-0700 directory under `/tmp`.

The Rust tests run as:

```text
trusted_service_transport_connects_and_authenticates_member
trusted_service_rejects_revoked_member_on_reconnect
trusted_service_tenant_sessions_are_isolated_across_private_dm_relays
live_provisioning_reaches_ready_after_nip42_and_member_only_read
live_provisioning_recovers_after_relay_outage
trusted_service_rejects_an_unapproved_dns_answer_before_socket_dial
```

The provisioning retry test scales only the tenant A TLS edge Deployment to
zero replicas and back to one, keeping its Service, TLS Secret, and signed
mapping generation unchanged. It verifies the first attempt is retryable, the
same identity is reused, then runs reconciliation and `drive` after relay
recovery before requiring the operation to reach `ready`.

`live_provisioning_reaches_ready_after_nip42_and_member_only_read` also calls
the internal managed-identity history reader and requires the exact seeded
event ID and original author after authorized kind-39000 discovery. It checks
that an undiscoverable channel is denied and that audit entries contain no
message body. It does not publish kind-9 messages: pinned Buzz evaluates the
current write role and channel state only after receiving a signed `EVENT`, so
there is no safe pre-sign permission check for this backend boundary.

Before the Rust tests, the harness attempts TCP to a forbidden public address
from the connector Pod and requires Hubble to record Cilium's egress denial.
`trusted_service_tenant_sessions_are_isolated_across_private_dm_relays` proves
transport to two independent relays, tenant-bound
private-channel reads, cross-tenant authentication denial, zero unauthorized
events, and rejected cross-channel publishes. The DNS rejection test temporarily
changes the controlled resolver so the canonical tenant A service name returns
the approved A address and unapproved B address. The Cilium policy allows both
edge destinations during that test, so network policy cannot hide a connector
failure. Hubble captures TCP and UDP flows; the test fails if a TCP flow is
observed during the DNS rejection window.

The member-revocation test confirms the already-authenticated member can read a
seeded private event, publishes an owner-signed kind 9031 event, then verifies
that the existing socket can no longer retrieve private events and a fresh
NIP-42 authentication is rejected. It records whether Buzz closes or fences
the existing socket.

## Runtime fixture and secret handling

The harness generates throwaway relay, operator, member, friend, database,
and TLS credentials. It creates communities through the pinned upstream
relay's NIP-98-authenticated operator community API. Separate NIP-43 kind 9030
owner identities enroll the test members; the seeder creates a native
NIP-29 private channel, adds its second member, publishes a kind 9 event, and
checks the private/hidden metadata. NIP-17 is not used.

Private credentials stay in mode-0600 temporary files and Kubernetes Secrets
inside the disposable cluster. The Rust test database is a PostgreSQL sidecar
in the same Pod as the connector, so `DATABASE_URL` remains loopback and passes
the server test's disposable-database guard. It is recreated before the DNS
test because the Rust tests write mapping audit records. No key, event body,
or private operator credential is copied into evidence. The output records
source, local image and test-binary digests, tool versions, sanitized test
results, selected manifests, Cilium policy, Hubble output, and a SHA-256
manifest.

## Latest executed qualification

Run on 2026-11-03 from RustShare commit
`f349605a13f3a1c076e47e246660fe9c1c370210`, using upstream Buzz commit
`eafd77e9f6e96052476bdd88be732c046fdb7ab3` and tree
`04e3a40545bd3b23510ec32b4c8cccee2acef7c5`. The locally built Buzz image
digest was
`sha256:501c3223fe04387dd0a3dd1dcf9912d2b4bf3deb8691ba6f073bc9511984b91e`.

- All six selected ignored Rust tests passed.
- The member flow exercised private-channel membership and reads. Relay outage,
  recovery, and kind-9031 revocation behavior were also exercised.

The sanitized evidence bundle is retained at
`/tmp/elembra-cilium-evidence-20261103180651-2276002`. Its `SHA256SUMS` file
hash is `b6ab377519dfd479b2a3a6eb4ab4557dd9b6cf23c913320a4fd1eac943f038e0`;
all 48 entries verified.
This evidence qualifies only the exercised disposable profile and tests; see
the limits below.

## Limits

- The test mapping signer is generated and trusted inside the Rust test. This
  harness does not prove production control-plane authorization, signer
  revocation, or deployment provenance acceptance.
- The DNS test proves rejection of a multi-address answer containing one
  unapproved target before a socket dial. It does not prove a temporal
  successive-query DNS rebinding race.
- The upstream Buzz image digest is a locally built artifact from the pinned
  source tree, not a published upstream release digest or production pin.
- The test does not enable Chat routing, real-user custody, KMS, identity
  provisioning/export, NIP-17, or production deployment.
- This is disposable connector qualification, not Gate I or production
  security acceptance. A successful test run does not authorize custody or
  release.
