# RustShare Desktop Phase 1 Test Matrix

| Feature | Category | Platform | Required Coverage | Pass Criteria |
| :--- | :--- | :--- | :--- | :--- |
| **Authentication** | E2E | macOS, Windows | 100% | Login, token save, logout, de-register. |
| **Workspace Selection** | E2E | macOS, Windows | 100% | Select directory, verify WorkspaceRoot in DB. |
| **Sync Root Selection** | UI | macOS, Windows | 100% | Sync and un-sync remote roots; verify on-disk changes. |
| **Local Create** | Integration | macOS, Windows | 100% | Create file; verify remote upload + ETag. |
| **Local Update** | Integration | macOS, Windows | 100% | Edit file; verify SHA-256 change + upload. |
| **Local Rename** | Integration | macOS, Windows | 100% | Rename file; verify remote rename (no re-upload). |
| **Local Delete** | Integration | macOS, Windows | 100% | Delete file; verify remote deletion. |
| **Remote Create** | Integration | macOS, Windows | 100% | Create remote; verify local download. |
| **Remote Update** | Integration | macOS, Windows | 100% | Remote change; verify local hash + metadata match. |
| **Conflict Handling** | Integration | macOS, Windows | 100% | Edit both simultaneously; verify conflict copy. |
| **Atomic Renaming** | Unit | macOS, Windows | 100% | No temp files left after download success. |
| **Pause/Resume** | Integration | macOS, Windows | 100% | Suspend loop; re-sync on resume. |
| **Restart Recovery** | Integration | macOS, Windows | 100% | Kill process; verify resume from last cursor. |
| **Large File Sync** | E2E | macOS, Windows | 100% | Sync 2GB+ file; verify integrity. |
| **Offline Mode** | E2E | macOS, Windows | 100% | Status set to "Offline"; queues paused. |
| **Long Path Support** | Platform | Windows | 100% | Sync files at > 260 character depth. |
| **Case-only Rename** | Integration | macOS, Windows | 100% | Rename `A.txt` to `a.txt`; verify sync server. |
| **Dotfile Sync** | Unit | macOS | 100% | Verify `.gitignore` exists after sync. |
| **Database Corruption** | Negative | macOS, Windows | 100% | Re-scan and re-attach remote roots on error. |
    
