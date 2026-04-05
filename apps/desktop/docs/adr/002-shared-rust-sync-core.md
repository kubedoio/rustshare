# ADR 002: Shared Rust Sync Core

## Status: Accepted
## Date: 2026-04-05

## Context
Desktop clients on macOS and Windows need identical sync behavior to avoid divergent platform-specific "quirks" in basic sync logic.

## Decision
The core sync engine (`sync-core`) will be a pure Rust crate, decoupled from UI code and platform-specific UI frameworks (e.g., Tauri, Flutter). 
- State management and scheduling live in `sync-core`. 
- Only platform-specific wrappers (e.g., `platform` crate) and the UI shell (e.g., `rustshare-desktop` app) are decoupled.

## Alternatives Considered
- **UI-integrated Sync**: Mixing sync logic with UI framework (Tauri commands) makes testing difficult and blocks future CLI-only usage.
- **FFI-based C++ Core**: Unnecessary for a Rust project.

## Consequences
- **Pros**: 100% logic parity between macOS and Windows, easier to unit test, portable to future platforms (Linux).
- **Cons**: Requires clear interface boundaries between the core and the UI.
