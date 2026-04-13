# RustShare Desktop Phase 1 Test Plan

## 1. Test Pyramid
- **Unit Tests (70%)**: Pure functions (path normalization, hash calculation, backoff logic, state transitions).
- **Integration Tests (20%)**: Inter-crate interaction (SQLite queries, local scanning, transfer scheduling).
- **End-to-End (E2E) Tests (10%)**: UI-to-Backend flows (Login, Workspace Setup, Full Sync lifecycle).

## 2. Mandatory Unit Test Areas
- `sync-core`: Queue prioritization logic.
- `file-ops`: Atomic rename implementation across partitions.
- `client-state`: SQL migration correctness.
- `platform`: Keyring access stubbing/mocking.

## 3. Mandatory Contract Tests
- `sync-protocol`: JSON serialization/deserialization of DeltaResponse.
- `backend-api`: Mocked server interactions for HTTP 200/400/409/500 codes.

## 4. Integration Tests
- **Sync Logic**: Verify local edit triggers an upload schedule.
- **Sync Logic**: Verify remote edit triggers a download schedule.
- **Path Stability**: Verify repeated uploads to the same canonical path update in place and do not create duplicate live rows.
- **Nested Paths**: Verify remote folder creation plus nested note/file creation materializes the full local directory path before file download.
- **No-op Re-uploads**: Verify identical-content uploads to the same canonical path do not create a new version.
- **Conflicts**: Force local and remote change; verify conflict copy creation.
- **Resumption**: Interrupt sync task; verify retry behavior with correct backoff.

## 5. End-to-End Platform Tests
- **Setup**: Login -> Select C:\Desktop\RustShare -> Select "Work" folder -> Sync 10 files.
- **Steady State**: Edit local file -> Verify remote version changed.
- **Steady State**: Edit the same local file repeatedly -> Verify remote file ID stays stable and no duplicate live path appears.
- **Steady State**: Move local folder -> Verify remote folder moved/renamed.
- **Steady State**: Create a nested remote folder and note -> Verify full local path exists with the expected content.
- **Offline**: Disconnect internet -> Verify "Offline" status -> Reconnect -> Verify "Catch up" sync.

## 6. Negative Tests
- **No Space**: Fail upload/download if local disk is full; report error.
- **Locked File**: Windows-only test for files open in Word/Excel; verify skip/retry.
- **Permission Denied**: Attempt sync of write-protected files; verify error log.
- **Corrupt DB**: Simulate corrupt SQLite; verify re-scan behavior.

## 7. Performance & Soak Tests
- **Deep Tree**: Sync a workspace with > 50 levels of nesting.
- **Large Root**: Sync 50,000 files in a single root.
- **Big File**: Sync 5GB file; verify RAM usage remains < 256MB.
    
