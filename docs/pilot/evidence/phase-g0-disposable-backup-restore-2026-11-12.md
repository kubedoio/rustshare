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

The repository's channel and conversation browser suites do exercise real signed messages, thread events, and Files attachments against the relay, and emit acceptance JSON containing event/channel identifiers. They are product acceptance drivers, not a recovery fixture API: they do not export the browser-held user signing keys to an independently controlled recovery input, persist an attachment content hash for post-restore comparison, or invoke the encrypted backup/restore flow. The existing restore integration test runs only synthetic PostgreSQL tables and volume files, with no Buzz relay container.

The old/new relay rollback also remains untested. It requires keeping one disposable Buzz PostgreSQL/object state mounted while starting both immutable image digests in sequence, then authenticating and querying the same signed events after each switch. Current live-conformance coverage starts one image in a fresh project and tears its volumes down; it does not preserve and compare state across image replacement. Adding that safely is a cross-stack drill requiring a durable non-secret fixture manifest and independently provided test-user key material; it is not a small change to the existing S3 backup test. Do not treat the passing synthetic fixture values as Chat/relay continuity evidence.
