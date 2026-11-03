# Disposable Cilium trusted-service qualification

This harness runs three ignored Rust connector tests against two isolated
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
trusted_service_tenant_sessions_are_isolated_across_private_dm_relays
trusted_service_rejects_an_unapproved_dns_answer_before_socket_dial
```

Before the Rust tests, the harness attempts TCP to a forbidden public address
from the connector Pod and requires Hubble to record Cilium's egress denial.
The first test proves transport to two independent relays, tenant-bound
private-channel reads, cross-tenant authentication denial, zero unauthorized
events, and rejected cross-channel publishes. The second temporarily changes
the controlled resolver so the canonical tenant A service name returns the
approved A address and unapproved B address. The Cilium policy allows both
edge destinations during that test, so network policy cannot hide a connector
failure. Hubble captures TCP and UDP flows; the test fails if a TCP flow is
observed during the DNS rejection window.

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
`045b7859a529b4e9d3e745a0ba62c0c6012b5139`, using upstream Buzz commit
`eafd77e9f6e96052476bdd88be732c046fdb7ab3` and tree
`04e3a40545bd3b23510ec32b4c8cccee2acef7c5`. Cilium `v1.16.4` reached `OK`
for Cilium, operator, Envoy and Hubble Relay. The isolated Buzz image digest
was `sha256:17e9f54cf4c567e535cf51707e67cbd5726af714da46bee61dcab1645f3944a3`.

- Member transport passed TLS/SNI/peer pin verification, NIP-11 identity
  validation, WebSocket upgrade, NIP-42 authentication and private-channel DM
  history retrieval.
- While a NIP-29 private-DM subscription was active, the test stopped and
  restored PostgreSQL. The session fenced during outage and remained fenced
  after recovery; mapping revocation was also verified.
- Tenant isolation passed: unauthorized histories returned zero events and
  private publishes in both cross-tenant directions were rejected.
- The poisoned multi-address DNS answer was rejected before a TCP socket dial.
- Hubble recorded the connector Pod's TCP SYN to `1.1.1.1:443` as
  `EGRESS DENIED` / `DROPPED`; the in-Pod probe received no response.
- All three selected ignored Rust tests passed. The disposable kind cluster
  and unique image tags were removed after the run.

The sanitized evidence bundle is retained on the test runner at
`/tmp/elembra-cilium-evidence-20261103112720-1637415`. Its `SHA256SUMS` file
hash is `e6bfdca9771bc3a6174f16e85dcd50e7e83c141d9701ae174a617dfaaa1034b5`;
all 35 entries verified.
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
