# RustShare Desktop Phase 1 Specification

| Status | Updated |
| :--- | :--- |
| **Version** | 1.3.0 |
| **Author** | Antigravity (Principal Systems Engineer) |
| **Date** | 2026-04-10 |
| **Updates** | Daemon architecture, Unix socket, CLI commands, deletion semantics, same-path update semantics |

## 1. Title
RustShare Desktop Sync Client - Phase 1

## 2. Status
Draft / Pending Review

## 3. Scope
Phase 1 focuses on building a lightweight, reliable desktop synchronization client for macOS and Windows. The goal is to provide fundamental "Workspace Sync" capabilities, allowing users to synchronize specific remote folders (Sync Roots) to a local directory (Workspace Root).

## 4. Non-goals
- **Native OS Integration**: No macOS File Provider or Windows Cloud Files API (Virtual Files).
- **Shell Extensions**: No Finder/Explorer right-click menus or status overlays.
- **Delta Sync**: No block-level transfers; files are synced in full.
- **Peer-to-Peer**: No LAN sync or direct device transfers.
- **Advanced Merging**: No automatic text merging; conflict copies only.
- **Mobile/Linux**: Support is reserved for future phases.
- **Editor Plugins**: No native Sublime/Notepad++ logic (only extension points).

## 5. Definitions
- **Workspace Root**: The primary local folder managed by RustShare.
- **Sync Root**: A remote folder subtree mapped into the Workspace Root.
- **Local State Store**: A local SQLite database tracking sync status, hashes, and metadata.
- **Sync Core**: The headless Rust engine responsible for the sync logic.
- **Conflict**: A state where both local and remote versions of a file have changed since the last known common state.

## 6. Personas / Use Cases
- **Alice (Software Engineer)**: Wants to sync her project files across her Mac and Windows machines without manual uploads.
- **Bob (Designer)**: Needs specific asset folders synced to his local machine for high-frequency editing, but doesn't want his entire account mirrored.

## 7. Functional Requirements
- **FR-1: Authentication**: User can sign in via the desktop application and establish a secure, durable device session.
- **FR-2: Workspace Selection**: User can select any local directory as the Workspace Root.
- **FR-3: Sync Root Configuration**: User can select/deselect remote folders to be synced locally.
- **FR-4: Local Change Detection**: Client detects additions, modifications, deletions, and renames in the local Workspace Root.
- **FR-5: Remote Change Detection**: Client polls the backend (or receives WebSocket notifications) for remote changes.
- **FR-6: Bi-directional Sync**: Client performs uploads and downloads to reconcile local and remote states.
- **FR-6a: Directory Mirroring**: Client synchronizes directory structure as first-class state, including empty folders.
- **FR-6b: Root Scoping**: Each sync root only mirrors the configured remote subtree.
- **FR-6c: Delete Propagation**: When a previously synced file is intentionally deleted on one side, that delete propagates to the other side instead of recreating the missing copy.
- **FR-6d: Delete Tombstones**: The local state store persists delete tombstones long enough to distinguish "intentional delete" from "new unsynced file."
- **FR-7: Persistence**: Client survives restarts, preserving the sync queue and state.
- **FR-8: Conflict Handling**: Client detects conflicts, creates "conflict copies," and alerts the user.
- **FR-9: Pause/Resume**: User can manually pause and resume sync operations.
- **FR-10: Activity Log**: User can view a history of recent sync operations and errors.
- **FR-11: Device Registration**: Client identifies itself to the backend with a stable ID.
- **FR-12: Large File Support**: Resumable transfers for large files (using existing backend capabilities).
- **FR-13: Background Daemon**: Sync runs as a background process managed via CLI.
- **FR-14: Daemon Lifecycle**: User can start, stop, and check status of the sync daemon.
- **FR-15: Sync Root CRUD**: User can add, remove, update, enable, and disable sync roots via CLI.
- **FR-16: CLI Communication**: CLI communicates with daemon via Unix socket (no TCP ports).

## 8. Non-functional Requirements
- **NFR-1: Determinism**: Given the same state, the sync algorithm must produce the same result.
- **NFR-2: Data Integrity**: No silent data loss; use checksums to verify transfers.
- **NFR-3: Restart Safety**: Sync operations are atomic or resumable; no partial/corrupt files left in target locations.
- **NFR-4: Observability**: Structured logging for all sync decisions and errors.
- **NFR-5: Performance**: Minimal CPU/Memory impact during idle states.
- **NFR-6: Security**: Redact sensitive data from logs; use OS-secure storage for tokens.

## 9. Platform Support Matrix
| Platform | Version | Support Level |
| :--- | :--- | :--- |
| macOS | 13+ (Ventura/Sonoma) | Tier 1 |
| Windows | 10/11 (x64) | Tier 1 |

## 10. Local Filesystem Behavior
- **Watching**: Uses `notify` crate (FSEvents on Mac, ReadDirectoryChangesW on Windows).
- **Atomic Writes**: Downloads are written to `.tmp` files and renamed upon completion.
- **Case Sensitivity**: Client must handle case-only renames (e.g., `README.md` -> `readme.md`) gracefully on case-insensitive filesystems.

## 11. Remote Sync Behavior
- **Polling/WS**: Initial sync uses full scan; steady state uses WebSocket notifications with a periodic poll fallback.
- **Direction**: Remote changes take precedence in tie-breaking scenarios where timestamps are identical but hashes differ (rare).
- **Path Preservation**: Relative paths inside a sync root are preserved on upload and download.
- **Same-path Update Semantics**: When a file already exists at the canonical remote path, uploads update that file in place instead of creating a duplicate row.
- **Same-content No-op**: Re-uploading identical bytes to the same canonical path is treated as a no-op for metadata and version history.
- **Ordering**: Directory creation runs before file transfer so nested content never depends on flattened uploads.
- **Deletion Model**: A missing file only means "delete the other side" when the client has prior synced state for that path. Otherwise it is treated as a new file on the side where it still exists.
- **Delete Idempotency**: Repeating a local delete for an already-missing local file or a remote delete for an already-missing remote file must be treated as success, not as a fatal error.
- **Tombstone Reconciliation**: If both sides are missing and a tombstone exists, the client keeps the path deleted. If one side later recreates the file, that side becomes the new source of truth for the recreated path.

## 12. Conflict Behavior
- **Policy**: Conservative.
- **Action**: Rename local file to `<filename> (Conflict <timestamp>).ext`.
- **Visibility**: Expose conflict event in UI for user resolution.
- **Deleted vs Recreated**: If a tombstoned path reappears on both sides independently, the client treats that as a conflict, not as silent resurrection.

## 13. Security Requirements
- **Transport**: TLS 1.2+ only.
- **Storage**: Keyring/SecKeychain for auth tokens.
- **Sanitization**: Logs must not contain filenames if they contain PII (optional toggle), and never contain tokens.

## 14. Error Handling Requirements
- **Exponential Backoff**: For network and server-side errors.
- **User Intervention**: Prompt user for permanent errors (e.g., Permission Denied, Disk Full).

## 15. Recovery Requirements
- **Database Corruption**: If the local state store is corrupt, perform a "re-index" sync (map local files to remote counterparts by hash).
- **Delete Recovery**: The client should preserve enough tombstone state to avoid recreating deleted files immediately after restart or after a transient backend listing inconsistency.

## 16. UI Requirements
- **Auth Screen**: Login/Logout.
- **Setup Wizard**: Workspace Root and Sync Root selection.
- **Dashboard**: Global status (Syncing, Up to Date, Error), Recent Activity.
- **Settings**: Path changes, bandwidth limits (deferred), pause/resume.

## 17. Logging and Diagnostics
- **Format**: JSON structured logs.
- **Level**: Default to INFO; DEBUG available via settings.

## 18. Packaging and Release
- **macOS**: `.dmg` or `.app` in a ZIP.
- **Windows**: MSI or EXE installer.

## 19. Acceptance Criteria
- [ ] Successful sync of 1000 small files.
- [ ] Successful sync of a 1GB file with simulated restart during transfer.
- [ ] Rename of a folder correctly propagates to remote.
- [ ] Conflict copy generated when local and remote change simultaneously.
- [ ] Deleting a previously synced local file removes the remote copy and does not recreate the file on the next sync cycle.
- [ ] Deleting a previously synced remote file removes the local copy and does not recreate the file on the next sync cycle.
- [ ] Repeating a delete after the target is already gone is idempotent and does not wedge the root in a retry loop.
- [ ] Creating a nested remote folder and note materializes the full local directory path before the file is written.
- [ ] Uploading new bytes to an existing canonical remote path updates the existing file in place and does not create a duplicate live row.
- [ ] Re-uploading identical bytes to the same canonical remote path does not create a new version or duplicate live row.

## 20. Open Questions / Deferred Items
- **Delta Sync**: Deferred to Phase 2.
- **Selective Sync (Fine-grained)**: Deferred to Phase 2 (Phase 1 is folder-level).
- **Hard/Symbolic Links**: Explicitly ignored in Phase 1.
