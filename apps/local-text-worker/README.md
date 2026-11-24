# Local Text worker

`elembra-local-text-worker` runs separately from the backend. It scans selected Markdown/text roots, stores content-addressed observations in a mode-0600 SQLite queue, and imports them through the Connector worker API. It watches for file changes and reconciles every 60 seconds by default. Restarting the process resumes queued imports.

Create a worker credential for a registered Connector instance. Windows uses Credential Manager, macOS uses Keychain, and Linux uses Secret Service when available. Store the one-time credential in that native store and run with `--credential-keyring`. Linux headless installations without Secret Service must explicitly use `--credential-file`; that fallback is supported only on Unix, requires a regular file with mode 0600, and is not equivalent to a native keyring. Configure `destinationFolderId` in the instance's `sourceConfig` to restrict imports to an approved Files folder; unset or `null` selects the account root.

For manual provisioning, use a Generic Password item with service `io.elembra.local-text-worker` and account equal to the Connector instance UUID. On Linux with Secret Service installed, `secret-tool store --label='Elembra Local Text' service io.elembra.local-text-worker account <connector-instance-uuid>` prompts for the secret without putting it in shell history.

```sh
elembra-local-text-worker \
  --instance-id <connector-instance-uuid> \
  --server https://elembra.example \
  --credential-keyring \
  --queue /var/lib/elembra/local-text.sqlite3 \
  --root /home/user/notes \
  --exclude 'private/**'
```

To migrate an existing Unix mode-0600 credential file, stop the Local Text worker and any process provisioning/rotating that Connector credential, then run once with `--migrate-credential-file /secure/path/connector-credential`. The worker writes the credential to the native store before removing the source file, and refuses to replace a different existing native credential. File removal does not guarantee forensic erasure on copy-on-write or solid-state storage. If Secret Service is unavailable, migration fails and the original file remains; use the explicit file fallback until an OS credential store is available.

Windows builds are produced by the `Local Text Windows` workflow as an unsigned ZIP artifact for controlled installation testing. Keep the credential in Windows Credential Manager; do not use a plaintext credential file. Connected Windows filesystem and backend acceptance is still required before #99 can close.

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
