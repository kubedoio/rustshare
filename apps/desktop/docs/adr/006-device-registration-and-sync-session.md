# ADR 006: Device Registration and Sync Session

## Status: Accepted
## Date: 2026-04-05

## Context
A desktop client must survive restarts and remain uniquely identified by the backend to avoid token reuse or re-registration.

## Decision
Desktop clients will register with a unique `DeviceId` upon first login.
- Identification: Stable hardware-bound ID (based on serial numbers or filesystem metadata, platform-specific).
- Sync Cursor: A version/token cursor for remote changes is persisted in the local state store.
- Device Session: Revocable by the backend if the device is lost/revoked.
- Local Storage: Auth tokens and `DeviceId` will be stored in OS-secure storage (Keychain/Data Protection API).

## Alternatives Considered
- **Stateless Polling**: Too inefficient; re-scans everything on start.
- **Short-lived Tokens**: Inconvenient for desktop apps (frequent logouts).

## Consequences
- **Pros**: Persistent identification, allows the backend to track which devices have "caught up" to specific change cursors.
- **Cons**: Requires stable hardware-based ID logic for each platform.
