# ADR 003: No Native Filesystem Provider in Phase 1

## Status: Accepted
## Date: 2026-04-05

## Context
Full native filesystem integration (macOS File Provider, Windows Cloud Files API) is complex and requires platform-specific development cycles.

## Decision
For Phase 1, we will not implement a virtual filesystem layer. 
- Files are materialized (downloaded) onto the local filesystem in a "Sync Root" folder. 
- No platform-native shell extensions or context menus (e.g., Finder right-click). 
- Files are managed by standard file IO; we watch the filesystem using standard APIs (`notify`).

## Alternatives Considered
- **File Provider / Cloud Files API**: High development cost for Phase 1.
- **FUSE-based (e.g., MacFUSE)**: Requires kernel extensions, complex for Windows and macOS.

## Consequences
- **Pros**: Speed to market, simplicity, minimal OS-level dependencies.
- **Cons**: No "smart-sync" (on-demand downloading) icons in Phase 1. All selected remote files are fully downloaded.
