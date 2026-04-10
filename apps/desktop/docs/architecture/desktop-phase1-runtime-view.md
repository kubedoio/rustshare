# RustShare Desktop Runtime View

## 1. Current Runtime Shape

The current macOS client is a CLI plus background daemon.

- `rustshare-desktop login` handles device pairing or explicit token login
- `rustshare-desktop daemon start` launches a detached background process
- the daemon runs the shared sync engine from `crates/sync-engine`
- the CLI talks to the daemon over a local Unix socket for status and control

There is no shipped GUI shell in the current release line.

## 2. Startup Sequence

### CLI path
1. Parse `--workspace`, `--server`, `--db-name`, and the subcommand.
2. Resolve `~/` in the workspace path and ensure the workspace exists.
3. Resolve the app-data directory through `platform::PathManager`.
4. Open the local SQLite state file.
5. Load the auth token from keychain, then fall back to `token.txt` for daemon-safe auth.

### Daemon start path
1. `daemon start` checks whether a daemon is already running.
2. It cleans up stale PID or socket files if needed.
3. It respawns the current binary with `daemon run`, forwarding workspace, server, DB name, and verbosity.
4. Stdout and stderr are redirected into `daemon.log`.
5. The background process writes `daemon.pid`, binds `daemon.sock`, and starts the sync manager.

On macOS, the runtime files live under:

```text
~/Library/Application Support/io.rustshare.RustShare/
```

## 3. Sync Loop

The shared sync engine runs the same high-level loop for every configured root:

1. Scan the local workspace tree.
2. Fetch the remote file list and folder tree for the configured root.
3. Load persisted file state, upload sessions, tombstones, and quarantined remote errors from SQLite.
4. Build a sync plan.
5. Execute in order:
   - create directories first
   - upload and download files
   - delete files
   - prune directories
6. Persist the resulting file state.
7. Return to idle until the next filesystem trigger or periodic sync pass.

## 4. Current Sync Semantics

The current client behaves like this:

- each sync root mirrors its configured remote subtree
- root `/` is a full-account mirror
- nested directories are created before file transfer starts
- empty directories are mirrored
- zero-byte files are uploaded as real files
- successful deletes are recorded as tombstones
- local `ENOENT` and remote `404/410` delete responses are treated as already-applied deletes
- broken remote downloads are quarantined per path so the rest of the root can keep syncing

## 5. Trigger Sources

The daemon is driven by a combination of:

- filesystem notifications from `notify`
- scheduled full sync passes
- daemon control RPC over the local socket
- startup recovery after daemon restart

The current release does not depend on a finished realtime push pipeline to stay correct. The full sync pass is still an important source of convergence.

## 6. Recovery Model

At startup and during repeated sync cycles, the client recovers from partial state with:

- persisted `file_states` in SQLite
- resumable upload sessions
- broken-remote quarantine records
- delete tombstones

If the daemon dies mid-sync, the next daemon process rebuilds state from the workspace, the remote root, and the local SQLite database, then plans the next converging action.

## 7. Operational View

The most useful live operational commands are:

```bash
rustshare-desktop daemon status
rustshare-desktop daemon logs --tail 200
rustshare-desktop daemon logs --follow
rustshare-desktop sync doctor
rustshare-desktop sync cleanup-remote <ROOT_ID>
```

`status` exists, but `daemon logs` and `sync doctor` are the real debugging surface today.
