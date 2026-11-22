# Local Text worker

`elembra-local-text-worker` runs separately from the backend. It scans selected Markdown/text roots, stores content-addressed observations in a mode-0600 SQLite queue, and imports them through the Connector worker API. It watches for file changes and reconciles every 60 seconds by default. Restarting the process resumes queued imports.

Create a worker credential for a registered Connector instance and store the one-time value in a file readable only by the worker account (`chmod 600`). Configure `destinationFolderId` in the instance's `sourceConfig` to restrict imports to an approved Files folder; unset or `null` selects the account root.

```sh
elembra-local-text-worker \
  --instance-id <connector-instance-uuid> \
  --server https://elembra.example \
  --credential-file /secure/path/connector-credential \
  --queue /var/lib/elembra/local-text.sqlite3 \
  --root /home/user/notes \
  --exclude 'private/**'
```

The worker imports eligible `.md`, `.markdown`, and `.txt` files by default. Hidden paths and common credential/key files are always excluded. Each file is limited to 4 MiB, each root scan to 256 MiB, and the durable queue to 1 GiB. Imports are one-way; deleting a local file does not delete its Elembra copy.

The connected process test launches the real worker against a loopback backend. Use only a disposable `rustshare_test*` database and a disposable S3-compatible bucket, then supply an owner token:

```sh
export RUSTSHARE_TEST_DISPOSABLE_DB=1
export RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1
export RUSTSHARE_CONNECTOR_TEST_SERVER=http://127.0.0.1:18333
export RUSTSHARE_CONNECTOR_TEST_OWNER_TOKEN=<disposable-owner-bearer-token>
export DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:15432/rustshare_test_local_text
SQLX_OFFLINE=true cargo test -p local-text-worker --test connector_worker_e2e -- --ignored --test-threads=1
```

The test imports and updates Markdown files, checks Files and provenance mappings, retries a previously acknowledged observation, reconciles a source edit after worker restart, verifies privacy exclusions, and proves a rotated credential cannot ingest later edits.
