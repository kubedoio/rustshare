<!--
This file follows the Keep a Changelog format.
See https://keepachangelog.com/en/1.1.0/ for details.
All notable changes to this project will be documented in this file.
-->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.2.0] - 2026-05-26

### Added

- Real-time WebSocket sync for module edits, with collaborative cursor support and conflict resolution.
- Note bundle architecture: notes now store as folders with `index.md`, `.rustshare.json`, and `events.jsonl`, enabling richer metadata and attachment handling.
- H1-based folder rename on save: editing the top-level heading in a note automatically renames its backing folder.
- Cross-user share isolation tests and public share validation.

### Changed

- **Dependency updates:**
  - Rust: `rand` 0.9 (with API migration), `keyring` 4.0, `uuid` 1.23, and other cargo dependency bumps.
  - Frontend: 37 npm dependency updates including SvelteKit, TanStack Query, and Excalidraw.
  - Docker: Node.js 22 base image update.

### Fixed

- **Meeting note creation:**
  - Fixed routing so folder-backed module artifacts (meetings, standups, kanban, notes, etc.) navigate to their dedicated editor instead of the generic file browser.
  - Added a title prompt in `MeetingsModuleView` instead of silently creating an "Untitled Meeting Note."
  - Fixed default meeting and standup `.rustshare.json` template schemas to match `MeetingMetadata` and `StandupMetadata` structs.
- **Frontend type safety:** Fixed pre-existing svelte-check and eslint errors across `MarkdownEditor`, `KanbanCardModal`, `Topbar.test`, admin forms, and dashboard widgets.
- **Backend tests:** Hardened integration tests for cross-user isolation, validated JSON doctests, and brainstorming handler formatting.
- **CI:** Added `RUSTFS_ALLOW_INSECURE_DEFAULT_CREDENTIALS` to integration test workflow; fixed DCO sign-off checks.

[0.2.0]: https://github.com/kubedoio/rustshare/compare/v0.1.1...v0.2.0
