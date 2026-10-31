# Disposable upstream Buzz interoperability run

This harness builds the unmodified `block/buzz` source at
`eafd77e9f6e96052476bdd88be732c046fdb7ab3` (tree
`04e3a40545bd3b23510ec32b4c8cccee2acef7c5`) using its own Dockerfile.
It copies the upstream `deploy/compose/compose.yml` into a private temporary
directory and starts **two** isolated Compose projects. Each has its own
PostgreSQL 17, Redis 7, Silo object store, git volume, relay key and closed
community. Only random loopback relay and TLS proxy ports are published. It never reads or
changes `config/buzz-compatibility.env` or a production Compose project.

From a clean RustShare worktree:

```sh
scripts/upstream-buzz-poc/run.sh
```

The runner verifies the source commit and tree, builds the image, installs the
frontend's locked Nostr test dependencies under its private temporary directory,
starts both relays, runs
`protocol.mjs`, prints sanitized provenance and conformance results, and
removes both projects and volumes. `BUZZ_SOURCE_DIR=/path/to/clean/checkout`
uses an already-fetched, exact-SHA checkout without modifying it. The printed
`upstream_image_id` is the locally built image config digest;
`upstream_image_build_digest` is Buildx's local build metadata digest. Neither
is a published registry digest. The temporary TLS certificate and CA path are
printed so the desktop GUI can trust this disposable WSS endpoint.

For the separate stock desktop GUI test, run with `POC_HOLD=1` in a terminal.
After protocol checks, the runner prints the WSS relay URLs and pauses. The protocol
test writes Alice's disposable `nsec` inside the runner's mode-`0700` temporary
directory with mode `0600` and prints its path while it waits.
Import it into `block/buzz-app` at
`662e91800af8204ec0e492522b8f23ef23b19baa`, confirm the displayed pubkey,
and verify that it receives relay messages and can publish a message. Start
the GUI with `G_TLS_CA_FILE` and `SSL_CERT_FILE` set to the printed
`tls_ca_path`. The current harness does not observe desktop-published event
IDs, so do not mark bidirectional same-event interoperability as passed from
this manual GUI step alone.
Press Enter
after the GUI test; the runner removes the key file, temporary configuration,
containers and volumes. Never upload the key file or a raw Compose config/log
as a CI artifact. The test data and identities are disposable.

The script does not claim GUI interoperability or revocation safety merely
because the relays start. Record every protocol and GUI result in the Phase 1
evidence report; a missing GUI run is **unverified**, not passed.
