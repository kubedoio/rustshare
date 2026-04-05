# ADR 001: Desktop Monorepo Boundary

## Status: Accepted
## Date: 2026-04-05

## Context
RustShare is a unified file sync product. We need to maintain a single source of truth for protocol logic, data models, and cryptography while allowing the desktop client and backend to iterate independently.

## Decision
The desktop client will live in the same repository as the web application and backend. 
- Desktop-specific code: `apps/desktop/` (UI shell, platform integration).
- Shared logic: `crates/` (sync-core, sync-domain, sync-protocol, client-state, file-ops, platform, test-support).
- Shared models between desktop and backend: Move to `crates/sync-protocol` and `crates/sync-domain`.

## Alternatives Considered
- **Separate Repositories**: Leads to "pinning" issues and protocol drift.
- **Single Crate**: Too bloated, makes testing and dependency management difficult across platforms.

## Consequences
- **Pros**: Shared types, shared crypto, simplified dependency management (single `Cargo.lock` at root).
- **Cons**: Need for strict CI to ensure desktop changes don't break backend and vice-versa.
