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

The runner verifies the source commit, tree and clean checkout, builds the
upstream image, installs this harness's locked Nostr protocol dependencies in
its private temporary directory, starts both relays, runs `protocol.mjs`, and
removes the projects, volumes and temporary key material.
`BUZZ_SOURCE_DIR=/path/to/clean/checkout` uses an already-fetched exact-SHA
checkout without modifying it. `upstream_image_id` is the local image config
digest and `upstream_image_build_digest` is Buildx's build digest; neither is a
published registry digest. The disposable TLS CA path is printed for the GUI
test.

The protocol seed adds two separate members to tenant B's relay, creates a
native kind-41010 private-channel DM, checks member discovery for kinds
39000/39001/39002 and the kind-39000 `private`/`hidden` tags, then confirms the
second member retrieves the exact kind-9 event ID. This loopback TLS proxy
harness proves upstream protocol behavior only; trusted private-service
connector and Cilium egress qualification use the separate ignored Rust test
`trusted_service_tenant_sessions_are_isolated_across_private_dm_relays`.

For the GUI test, use a private desktop profile and a real unlocked OS
credential service (the upstream client must use its normal secure-storage
implementation). Run the harness interactively with `POC_HOLD=1`. It pauses
after the protocol checks, before Bob's revocation. It creates Alice's
disposable `nsec` in the mode-`0700` run directory as a mode-`0600` file and
prints its path, public key, relay URL and channel ID. Import that key into the
unmodified desktop at `662e91800af8204ec0e492522b8f23ef23b19baa`; verify the
displayed public key and connect to the printed relay and community. Set
`G_TLS_CA_FILE` and `SSL_CERT_FILE` to the printed CA when starting the desktop.

While the relay harness is held, enter these commands in its terminal as each
step is completed:

1. `SEND <unique-text>` publishes from the POC client. Confirm that exact text
   appears in the desktop; retain the printed event ID.
2. Send a different unique text from the desktop. Enter `WAIT <unique-text>`;
   retain the retrieved event ID and public key and compare the ID with the
   desktop's displayed event when available.
3. Restart the desktop, reconnect, and enter `HISTORY <event-id>` to confirm
   persistence and the same author key.
4. Check the desktop has no duplicate identity or unexpected community
   enrollment, then enter `EXIT` to continue removal and cleanup tests.

The terminal output and screenshots must omit private keys and message text
that is not needed as evidence. Never upload the `nsec`, its file, raw Compose
environment/logs, private profile or keyring as CI artifacts. If the GUI cannot
be exercised, report desktop interoperability as **unverified**.

The script does not claim GUI interoperability or revocation safety merely
because the relays start. Record every protocol and GUI result in the Phase 1
evidence report; a missing GUI run is **unverified**, not passed.

For the full GUI checklist and sanitized-only evidence format, see
[`desktop-acceptance.md`](desktop-acceptance.md). The structured evidence
collector is `desktop-evidence.mjs`; it does not capture screenshots, read
keychain data, or accept secret-bearing fields.
