# Phase G0 disposable backup and restore evidence

**Result:** PASS for the local backup pipeline test; NOT a production or off-host disaster-recovery qualification.

- RustShare source: `2078abae9f96dcfc4f62e74c72baba99a63c2c5e` (main)
- Test: `scripts/test-elembra-offsite-backup.sh`
- Run date: 2026-11-12 UTC
- Result: exit 0. The script created uniquely named disposable Docker resources, encrypted a `--with-chat` bundle to a local RustFS S3-compatible service, checked its checksum and lifecycle rule, decrypted it using an independently generated age identity, changed the PostgreSQL fixture rows and RustFS fixture objects, then restored and verified their prior values. The failure upload path emitted the expected `/fail` heartbeat.
- Final rerun object: `elembra/20261112T182316Z.tar.gz.age`, SHA-256 `6d14108e388d3214fa12e754938fe4d37e4959cce63c67e515cc91a1c6037520` (the disposable S3 container and object were deleted by cleanup).
- Test runtime dependency: age/age-keygen v1.2.1 installed under `/tmp` for this run; no system package was changed.
- Cleanup: the test trap removed its Docker Compose project, containers, volumes, heartbeat server and temporary files.

## Scope limits

This test uses synthetic fixture tables and object files. It does not start the Buzz relay or observer, seed real Elembra users/channels/messages/threads/attachments, assert relay identity/event continuity, exercise the old/new relay image rollback, or restore from an independently administered S3 account. Its local S3 service and application fixtures share the same machine and Docker host. The generated age identity is independent of the backup writer, but S3 read access uses the test's writer credential.

Therefore the run verifies encryption, upload, bundle integrity, failure signaling, and database/object-volume restore mechanics only. Pilot recovery still requires a separate storage account/failure domain, distinct recovery read credentials, independently held age identity, representative application data, and a relay image rollback drill against that restored state.

### Why existing acceptance does not close this gap

The repository's channel and conversation browser suites do exercise real signed messages, thread events, and Files attachments against the relay. Specifically, `frontend/tests/chat-conversation-acceptance.e2e.ts` decrypts the browser key into in-memory `user.secretKey`, runs NIP-42 relay queries, and writes channel/event IDs to evidence JSON. `frontend/tests/chat-channel-acceptance.e2e.ts` uploads Files objects and verifies authorization, but does not retain attachment content hashes for a later restore comparison. Neither suite exports browser-held test signing keys to an independently controlled recovery input or invokes the encrypted backup/restore flow. The existing `scripts/test-elembra-offsite-backup.sh` Compose file starts only PostgreSQL and RustFS for Elembra/Buzz, with no RustShare app, Buzz relay, Redis, or observer.

The old/new relay rollback also remains untested. `scripts/run-buzz-conformance.sh` uses the compatibility manifest to start one image in a fresh project, runs its fixed conformance test, then always tears the volumes down. It does not preserve and compare state across image replacement. A real recovery gate needs a separate orchestration path that: (1) provisions the full app/relay/observer stack in a unique project; (2) captures signed event snapshots and attachment hashes in a durable non-secret manifest while placing test signing keys in separately supplied, private recovery material; (3) backs up and restores all stores; (4) checks the same events (including authors/signatures), channel permissions/history and attachment bytes; and (5) recreates only the relay with each exact immutable image while retaining Buzz PostgreSQL/object volumes, verifying observer replay after each switch. This involves browser-key custody and coordinated stateful service transitions, so it is not a safe small extension to the existing S3 test. Do not treat the passing synthetic fixture values as Chat/relay continuity evidence.


## Follow-up: 2026-11-15 cross-stack disposable drill

The limitations above describe the November 12 synthetic test. A later isolated
`scripts/run-chat-recovery-drill.sh` run passed at source
`eb65492f6c59fc693f20c6b72983b148ef673c81`. Its evidence is retained on the
qualification host at `/srv/data01/elembra-g0-recovery-evidence-20261115-final`.
Independent evidence review confirmed:

- Channel browser scenarios: 13/13; conversation scenarios: 9/9.
- Exact database/object fingerprints and API event IDs, authors, threads and
  attachments match before and after restore. API projection SHA-256:
  `7a6fc6f3dc1285133e5a96e05dce93c544d27eb3b5445e8cc311672b42c523dd`.
- Private history and removed-member write denial, attachment bytes and Files
  authorization, signed-event verification and observer catch-up passed.
- Candidate/old-relay rollback retained history; replay retained 27/27 events.
- Cleanup left zero drill-project containers, volumes or networks.
- `result.env` SHA-256:
  `f0d6713288c43aff533ab4f4bda7e02543a27f5a2f4ad84435266ad947415e1b`.

All recovery harness, history-probe and acceptance files are byte-identical to
merged PR #493 head `c44afdca83594a2d98369e3f7dd5d2ff6785051d`. That head also
incorporates seven PR #499 frontend capability/UI changes, so this run qualifies
the tested application artifacts and recovery harness, not the exact final
application artifacts. It does not qualify off-host encrypted recovery,
independently held recovery credentials, browser-held signing-key recovery or a
real pilot host. Those release gates remain open; the production relay pin is
unchanged.
