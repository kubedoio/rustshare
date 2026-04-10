# RustShare Desktop Changelog

## 0.3.0 - 2026-04-09

### Changed
- The desktop release is now versioned as `0.3.0`.
- The repository README and desktop client docs were refreshed to describe the current shipped CLI and daemon behavior on macOS.
- The architecture and runtime docs now reflect the live shared sync engine path, the background daemon model, and the actual app-data storage locations used by the client.

### Fixed
- Zero-byte files now upload as real synced files instead of being skipped and re-planned on every daemon cycle.
- Previously synced directories now propagate deletion instead of being recreated immediately from the other side.

### Verification
- `PATH=/opt/homebrew/bin:$PATH /opt/homebrew/bin/cargo test -p sync-engine --lib -- --nocapture`
- `PATH=/opt/homebrew/bin:$PATH /opt/homebrew/bin/cargo test -p rustshare-desktop --bin rustshare-desktop -- --nocapture`
- `PATH=/opt/homebrew/bin:$PATH /opt/homebrew/bin/cargo build -p rustshare-desktop`
- `PATH=/opt/homebrew/bin:$PATH /opt/homebrew/bin/cargo build --release -p rustshare-desktop`
- Live root verification on `8cc8ba70-adb6-4898-acc0-6c04328b8157`: created a disposable zero-byte file, observed one upload, confirmed remote `size: 0`, and confirmed subsequent daemon cycles returned `No sync operations needed`.

## 0.2.0 - 2026-04-09

### Added
- Explicit directory sync planning for local and remote folders.
- Empty-directory mirroring inside each configured sync root.
- Versioned macOS packaging guidance for release artifacts.
- `sync doctor` for root health checks, quarantined remote error reporting, and `/` root diagnostics.
- `sync doctor --clear-quarantine` to reset broken-remote quarantine records without editing SQLite by hand.
- `sync cleanup-remote` to dry-run or apply stale remote metadata cleanup after a fresh missing-file recheck.
- `daemon logs --tail` and `daemon logs --follow` for faster daemon troubleshooting from the CLI.

### Changed
- Sync roots are now scoped to their configured remote subtree instead of reading unrelated remote content.
- Directory structure is created before file upload and download, so nested paths are preserved end to end.
- Uploads now resolve the correct remote parent folder chain instead of flattening files into the sync root.
- The desktop package version was bumped from `0.1.0` to `0.2.0`.
- The daemon now detaches cleanly on macOS when started in the background, instead of dying with the parent shell.

### Fixed
- macOS client sync no longer copies only the files inside nested directories while skipping the directories themselves.
- Zero-byte files now upload as real synced files instead of being skipped and re-planned on every daemon cycle.
- `daemon start` now preserves the active `--workspace`, `--server`, `--db-name`, and `--verbose` settings when launching the background sync process.
- Desktop login now persists a daemon-readable token fallback, so background sync can still authenticate when keychain lookup is unavailable.
- Relative sync root paths now resolve against the configured workspace, and `sync update --local-path` keeps SQLite and `config.toml` aligned.
- Unchanged files are no longer re-uploaded on every filesystem event because synced file state timestamps now use consistent second precision.
- Remote deletes now use the live API path instead of being treated as "would delete" no-ops in the shared engine.
- Root-level `/` mirrors no longer stall forever on broken remote download metadata. Missing blobs are quarantined per path so the rest of the sync can continue.

### Verification
- `PATH=/opt/homebrew/bin:$PATH /opt/homebrew/bin/cargo test -p client-state --lib -- --nocapture`
- `PATH=/opt/homebrew/bin:$PATH /opt/homebrew/bin/cargo test -p rustshare-desktop --bin rustshare-desktop -- --nocapture`
- `PATH=/opt/homebrew/bin:$PATH /opt/homebrew/bin/cargo test -p sync-engine --lib -- --nocapture`
- `PATH=/opt/homebrew/bin:$PATH /opt/homebrew/bin/cargo build --release -p rustshare-desktop`
- `PATH=/opt/homebrew/bin:$PATH /opt/homebrew/bin/cargo check -p rustshare-desktop`
- Live root verification on `8cc8ba70-adb6-4898-acc0-6c04328b8157`: created a disposable zero-byte file, observed one upload, confirmed remote `size: 0`, and confirmed subsequent daemon cycles returned `No sync operations needed`.
