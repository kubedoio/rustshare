# RustShare Desktop for macOS - Installation Guide

This guide explains how to install and run the current RustShare macOS client from this repository.

## Current State

The repository already contains a working macOS desktop target, but it is important to be precise about what is available today:

- The active desktop crate lives in `apps/desktop`.
- The current macOS client is a CLI binary named `rustshare-desktop`.
- `cargo check -p rustshare-desktop` succeeds in the workspace.
- Authentication is pairing-first from the command line.
- Tokens are stored through the system keyring, which maps to macOS Keychain on Mac.
- The repository does not yet contain a finished `.app`, `.pkg`, or notarized `.dmg` release pipeline.
- The optional `tauri` dependency in `apps/desktop/Cargo.toml` is a placeholder and is not currently wired into a shipping GUI shell.

If you need a proper drag-and-drop Mac app for external distribution, there is still packaging work to do. If you need to build, install, and run the client on a Mac today, the source-build path below is the correct path.

## Supported macOS Version

The desktop Phase 1 spec targets:

- macOS 13+ (Ventura/Sonoma)

## What This Install Gives You

After following this guide, you will have:

- a locally installed `rustshare-desktop` binary
- a local workspace root for synced content
- a persisted device ID and local SQLite state
- a device token stored in macOS Keychain
- a foreground sync daemon you can run from Terminal

## Prerequisites

Install the following first:

- Xcode Command Line Tools
- Rust stable via `rustup`
- access to a RustShare server
- access to the RustShare web UI for that server
- an authenticated browser session in that web UI when pairing the device

Commands:

```bash
xcode-select --install
curl https://sh.rustup.rs -sSf | sh
source "$HOME/.cargo/env"
rustup default stable
rustc --version
cargo --version
```

## Build the macOS Client

From the repository root:

```bash
cargo build --release -p rustshare-desktop
```

The compiled binary will be created at:

```text
target/release/rustshare-desktop
```

## Install the Binary Locally

You can run the binary directly from `target/release`, but installing it into your shell `PATH` is more convenient.

Example using a user-local bin directory:

```bash
mkdir -p "$HOME/.local/bin"
cp target/release/rustshare-desktop "$HOME/.local/bin/rustshare-desktop"
chmod +x "$HOME/.local/bin/rustshare-desktop"
```

If `~/.local/bin` is not already in your shell `PATH`, add this to `~/.zshrc`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Reload your shell:

```bash
source ~/.zshrc
```

Verify the install:

```bash
rustshare-desktop --help
```

## First-Time Setup

### 1. Choose your workspace root

By default, the client uses:

```text
~/RustShare
```

You can override that with `--workspace`.

### 2. Pair the device

The current client creates a short-lived approval link and waits for web approval:

```bash
rustshare-desktop --server http://localhost:8080 login
```

The CLI prints:

- a full approval URL
- a note that the approval URL is valid for 5 minutes
- a reminder to open that URL from a browser where you are already signed in to the RustShare web UI

Example output:

```text
Approve this device in RustShare:
http://localhost:5173/device/approve?device_code=...

This approval link is valid for 5 minutes.
Open it from an authenticated RustShare web UI session to approve this device.

Waiting for approval...
```

Notes:

- The compiled client defaults to `https://api.rustshare.io` if you do not pass `--server`.
- For local development, you will usually want to override `--server`.
- The approval link is not the final device token. It is only a short-lived pairing token.
- After approval, the server issues the real device token and the desktop stores it in macOS Keychain automatically.

Direct token login still exists as an explicit fallback for admin or debugging use:

```bash
rustshare-desktop --server http://localhost:8080 login --token <TOKEN>
```

### 3. Add a sync root

The client maps a remote path to a local path inside the workspace root:

```bash
rustshare-desktop --server http://localhost:8080 sync add <REMOTE_PATH> <LOCAL_PATH>
```

Example:

```bash
rustshare-desktop \
  --workspace "$HOME/RustShare" \
  --server http://localhost:8080 \
  sync add "/teams/engineering" "engineering"
```

In that example:

- `/teams/engineering` is the remote folder path
- `engineering` becomes the local folder under `~/RustShare`

### 4. Start the sync daemon

```bash
rustshare-desktop --workspace "$HOME/RustShare" --server http://localhost:8080 daemon
```

The current daemon runs in the foreground. Stop it with `Ctrl+C`.

### 5. Check status

```bash
rustshare-desktop status
```

### 6. List configured sync roots

```bash
rustshare-desktop sync list
```

## Runtime Behavior on macOS

The current implementation behaves like this:

- Workspace root defaults to `~/RustShare`
- app state is stored under the platform app-data directory for `io/rustshare/RustShare`
- the local SQLite database is created there using the default name `rustshare.db`
- a persistent `device_id` file is created there as well
- credentials are stored in the system keychain, not in plaintext config
- file watching relies on macOS filesystem notifications through the Rust `notify` crate

On a typical macOS system, the app-data directory resolves under:

```text
~/Library/Application Support/io.rustshare.RustShare/
```

## Troubleshooting

### `rustshare-desktop: command not found`

Your shell cannot find the installed binary. Confirm that:

- the binary exists in `~/.local/bin/rustshare-desktop`
- `~/.local/bin` is in your `PATH`
- your shell session has been reloaded

### The client connects to the wrong server

The current CLI default is:

```text
https://api.rustshare.io
```

If you are working against local or staging infrastructure, always pass `--server`.

### Login works but sync does not start

Make sure you have:

- completed the pairing flow or used the explicit `--token` fallback
- added at least one sync root with `sync add`
- started the daemon explicitly with `daemon`

### The approval link expires before I use it

The pairing link is only valid for 5 minutes. Run `rustshare-desktop login` again to request a fresh link, then open it from an already authenticated RustShare web UI session.

### macOS warns about an unsigned binary

That is expected for a locally built binary that has not been signed or notarized. For local development, you can run the binary directly from Terminal. For wider distribution, use the packaging guidance below and add signing/notarization.

## Internal Packaging for macOS

If you want to share the client internally, the current repo supports a basic binary packaging flow, not a polished Mac app installer flow.

Build the release binary:

```bash
cargo build --release -p rustshare-desktop
mkdir -p dist/macos
cp target/release/rustshare-desktop dist/macos/
```

Optional signing:

```bash
codesign --force --sign "Developer ID Application: YOUR TEAM" dist/macos/rustshare-desktop
```

At this stage you can distribute:

- the standalone binary
- a `.tar.gz` archive containing the binary
- a manually assembled DMG if your team adds the missing wrapper/bundling steps

## What Is Still Missing for a Full Mac End-User Installer

The repository does not yet automate these pieces:

- `.app` bundle creation
- app icon and `Info.plist` packaging
- signed drag-and-drop DMG generation
- notarization and stapling
- LaunchAgent or background service setup for daemon auto-start
- GUI onboarding for login, workspace selection, and sync root configuration

If the goal is a consumer-friendly macOS installer, those items are the next workstream rather than installation steps a user can perform today.

## Related Docs

- `apps/desktop/docs/distribution/build-and-package.md`
- `apps/desktop/docs/specs/desktop-phase1-spec.md`
- `apps/desktop/docs/architecture/desktop-phase1-architecture.md`
