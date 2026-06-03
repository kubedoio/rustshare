# ADR-001: RustShare Vault Sync Product Scope

## Status

Accepted.

## Context

RustShare needs to support users who use Obsidian as a local client application. These users want their local vault files, Markdown notes, attachments, diagrams, and folder structures to be synchronized into RustShare.

The feature must not turn RustShare into a clone of Obsidian’s own paid sync service. RustShare should be positioned as a self-hosted company memory, file, and vault synchronization backend that can support local Markdown-based vaults.

## Decision

Implement a generic feature called:

```text
RustShare Vault Sync
```

Add an adapter for local Obsidian vault folders:

```text
adapter_type = "obsidian_vault"
```

RustShare Vault Sync will preserve the local vault as a file tree. RustShare will not convert Obsidian vault files into internal RustShare Notes records.

## Non-goals

The first implementation is not intended to support:

```text
- real-time collaborative editing
- full Obsidian mobile support
- automatic Markdown merge
- official plugin marketplace submission
- .obsidian configuration sync by default
- claim of official Obsidian affiliation
- reverse engineering of Obsidian Sync
```

## Initial MVP Scope

The MVP must support:

```text
- desktop-first Obsidian plugin
- one user syncing one vault
- manual sync
- periodic sync
- Markdown files
- attachments
- folder preservation
- RustShare manifest endpoint
- SHA-256 based change detection
- server revision checks
- 409 Conflict for stale writes
- conflict copy creation instead of silent overwrite
```

## Consequences

This design keeps RustShare independent from Obsidian internals. It also creates a future-proof foundation for other vault-style clients such as Markdown editors, local documentation tools, or code editors.

## Acceptance Criteria

```text
- Feature is named RustShare Vault Sync.
- Obsidian support is implemented as an adapter.
- User content is stored as files/folders, not converted into proprietary RustShare-only note records.
- Attachments remain visible as files.
- RustShare can index and preview content but must preserve original files.
- Product framing follows ADR-004.
```

> Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
