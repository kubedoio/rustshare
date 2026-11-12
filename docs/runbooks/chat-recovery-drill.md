# Disposable Chat recovery drill

`scripts/run-chat-recovery-drill.sh` runs the repository backup and restore
workflow against a fresh source worktree and a uniquely named Docker Compose
project. It never reads the caller's `.env` or `.elembra` state and refuses to
reuse a project that already has Compose-labeled containers, volumes, or
networks. All published host ports bind to `127.0.0.1`; the relay, backend,
observer, and browser use one generated `localhost:<port>` URL so Buzz's
Host-derived community identity stays consistent.

## Run

Run from a clean committed RustShare checkout with Docker Compose, Node/npm,
Python, curl, OpenSSL, and `sha256sum` available:

```bash
scripts/run-chat-recovery-drill.sh
```

The default relay references are immutable digests for the supported old
v1alpha1 image and the candidate image. They can be overridden with
`ELEMBRA_RECOVERY_OLD_RELAY_IMAGE` and
`ELEMBRA_RECOVERY_NEW_RELAY_IMAGE`, but both values must be different
`@sha256:` references. The drill builds the current RustShare source and
observer in its child worktree. It initializes Chat, runs the API dogfood and
browser channel/conversation suites, fingerprints database and object state,
backs up, removes only its own Compose volumes, restores, verifies Files bytes
and authorization, compares signed NIP-42 relay history and Chat API results,
then switches the relay candidate and rolls back while checking state equality.

The result summary and public-key/event-ID test evidence are written to a
mode-0700 directory under `/tmp` by default. Set
`ELEMBRA_RECOVERY_EVIDENCE_DIR` to select another local evidence directory.
Database dumps, object archives, generated Chat service keys, browser state,
cookies, passwords, and full command logs remain in a mode-0700 temporary
directory and are deleted by the exit cleanup. The local `backup-stack.sh`
bundle is plaintext; it contains only disposable fixture data and is not an
off-host or encrypted-backup qualification.

## Evidence limits

A successful run proves a local disposable PostgreSQL/RustFS/Buzz restore and
same-state relay image switch for that exact source checkout and image pair.
It does not prove external backup durability, disaster-recovery account/key
custody, cross-region restore, or independent institutional recovery. Browser
acceptance creates ephemeral test identities; the browser-held signing keys
are not exported or recovered by this drill. Relay event continuity is checked
with public signatures and the temporary deployment service identity, not the
browser private keys. Preserve production recovery keys and verify an
independent off-host restore separately before claiming production DR
readiness.
