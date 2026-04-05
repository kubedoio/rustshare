# RustShare Desktop Runtime View

## 1. Startup Sequence
1. **Initialize Shell UI**: Load the application window and system tray.
2. **Authorize Session**: Check for valid tokens in secure storage.
3. **Register Device**: If login is present, ensure `DeviceId` is registered with the backend.
4. **Boot Sync Core**: Initialize the `SyncCoreWorker`. 
5. **Initial Scan**: Perform a "deep scan" of the Workspace Root to verify parity with the local `ClientStore`.
6. **Start Watchers**: Begin monitoring the Workspace Root via `notify`.
7. **WebSocket Connect**: Establish a real-time connection to the backend for change notifications.

## 2. Steady-State Sync Loop
The `SyncCoreWorker` operates in a non-blocking loop:
1. **Idle**: Wait for a trigger (Local change detection, Remote notification, Poll timeout).
2. **Processing**: Identify files to upload or download.
3. **Active**: Execute transfers. 
4. **Completed**: Update metadata and UI status.

## 3. Pause/Resume
- **Pause**: Suspends the `SyncCoreWorker` and closes the WebSocket connection.
- **Resume**: Re-triggers the Initial Scan to catch up on any missed changes.

## 4. Shutdown Behavior (Safe Exit)
- Gracefully cancel active transfers.
- Commit current sync cursors and queue status to the Local State Store.
- Remove temporary `.tmp` files created for downloads.

## 5. Crash Recovery
- At startup, any `.tmp` files found in the Workspace Root are purged or resumed.
- Identify "half-sync" entries in the database and re-calculate hashes. 
- Re-check with the backend to ensure no data was lost during the crash.

## 6. Concurrency Model
- **UI Thread**: Responsive shell.
- **Sync Core**: Dedicated async task for planning and worker lifecycle.
- **Transfer Workers**: Bounded thread pool for heavy IO (uploads/downloads).
    
