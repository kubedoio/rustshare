import { getWebSocketClient, disconnectWebSocket } from "./client";
import { queryClient } from "$lib/query-client";
import {
  replicationStore,
  type ReplicationStatus,
} from "$lib/stores/replication";
import { toastStore } from "$lib/stores/toast";
import type { WebSocketEvent } from "./events";

let currentUserId: string | null = null;
let eventHandlersRegistered = false;

/**
 * Initialize WebSocket connection for the authenticated browser session.
 * Sets up all event handlers for real-time sync
 */
export async function initializeWebSocket(
  token: string | null,
  userId: string,
): Promise<void> {
  currentUserId = userId;
  const wsClient = getWebSocketClient();

  // Register event handlers only once
  if (!eventHandlersRegistered) {
    registerEventHandlers(wsClient);
    eventHandlersRegistered = true;
  }

  try {
    await wsClient.connect(token);
    console.log("[WebSocket Manager] Connected successfully");
  } catch (error) {
    console.error("[WebSocket Manager] Failed to connect:", error);
    throw error;
  }
}

/**
 * Disconnect WebSocket and cleanup
 */
export function cleanupWebSocket(): void {
  disconnectWebSocket();
  currentUserId = null;
  eventHandlersRegistered = false;
}

/**
 * Register all event handlers for real-time sync
 */
function registerEventHandlers(
  wsClient: ReturnType<typeof getWebSocketClient>,
): void {
  // File events
  wsClient.on("FileUploaded", handleFileUploaded);
  wsClient.on("FileModified", handleFileModified);
  wsClient.on("FileRenamed", handleFileRenamed);
  wsClient.on("FileMoved", handleFileMoved);
  wsClient.on("FileDeleted", handleFileDeleted);
  wsClient.on("FileRestored", handleFileRestored);

  // Folder events
  wsClient.on("FolderCreated", handleFolderCreated);
  wsClient.on("FolderRenamed", handleFolderRenamed);
  wsClient.on("FolderMoved", handleFolderMoved);
  wsClient.on("FolderDeleted", handleFolderDeleted);

  // Share events
  wsClient.on("ShareCreated", handleShareCreated);
  wsClient.on("ShareRevoked", handleShareRevoked);
  wsClient.on("ShareUpdated", handleShareUpdated);
  wsClient.on("ReplicationStateChanged", handleReplicationStateChanged);
}

// Helper to check if event is from current user
function isOwnEvent(event: WebSocketEvent): boolean {
  return event.user_id === currentUserId;
}

function isOwnOrSystemEvent(event: WebSocketEvent): boolean {
  return !event.user_id || event.user_id === currentUserId;
}

// File Event Handlers
function handleFileUploaded(event: WebSocketEvent): void {
  console.log("[WebSocket Manager] FileUploaded event:", event);

  // Backend doesn't send file details in the event payload for generic events
  // We just have aggregate_id (file ID) and need to invalidate queries to refetch
  const fileId = (event as any).aggregate_id;
  console.log("[WebSocket Manager] File ID from aggregate_id:", fileId);

  // Invalidate ALL folder contents queries to refetch
  console.log("[WebSocket Manager] Invalidating all folder-contents queries");
  queryClient.invalidateQueries({ queryKey: ["folder-contents"] });

  if (!isOwnEvent(event)) {
    toastStore.show(`New file was uploaded`, "info");
  }
}

function handleFileModified(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate file details and folder contents
  queryClient.invalidateQueries({ queryKey: ["file", payload.file_id] });
  queryClient.invalidateQueries({ queryKey: ["folder-contents"] });

  if (!isOwnEvent(event)) {
    toastStore.show(
      `File "${payload.file_name}" was modified (v${payload.version})`,
      "info",
    );
  }
}

function handleFileRenamed(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate file details and folder contents
  queryClient.invalidateQueries({ queryKey: ["file", payload.file_id] });
  queryClient.invalidateQueries({ queryKey: ["folder-contents"] });

  if (!isOwnEvent(event)) {
    toastStore.show(
      `File renamed from "${payload.old_name}" to "${payload.new_name}"`,
      "info",
    );
  }
}

function handleFileMoved(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate both old and new folder contents
  queryClient.invalidateQueries({
    queryKey: ["folder-contents", payload.old_folder_id],
  });
  queryClient.invalidateQueries({
    queryKey: ["folder-contents", payload.new_folder_id],
  });
  queryClient.invalidateQueries({ queryKey: ["file", payload.file_id] });

  // Invalidate root folders if needed
  if (!payload.old_folder_id || !payload.new_folder_id) {
    queryClient.invalidateQueries({ queryKey: ["folder-contents", null] });
  }

  if (!isOwnEvent(event)) {
    toastStore.show(`File "${payload.file_name}" was moved`, "info");
  }
}

function handleFileDeleted(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate file details and folder contents
  queryClient.invalidateQueries({ queryKey: ["file", payload.file_id] });
  queryClient.invalidateQueries({
    queryKey: ["folder-contents", payload.folder_id],
  });
  replicationStore.remove(payload.file_id);

  // Invalidate root if folder_id is null
  if (!payload.folder_id) {
    queryClient.invalidateQueries({ queryKey: ["folder-contents", null] });
  }

  if (!isOwnEvent(event)) {
    toastStore.show(`File "${payload.file_name}" was deleted`, "info");
  }
}

function handleFileRestored(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate file details and folder contents
  queryClient.invalidateQueries({ queryKey: ["file", payload.file_id] });
  queryClient.invalidateQueries({
    queryKey: ["folder-contents", payload.folder_id],
  });

  if (!payload.folder_id) {
    queryClient.invalidateQueries({ queryKey: ["folder-contents", null] });
  }

  if (!isOwnEvent(event)) {
    toastStore.show(`File "${payload.file_name}" was restored`, "success");
  }
}

// Folder Event Handlers
function handleFolderCreated(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate parent folder contents and folder tree
  queryClient.invalidateQueries({
    queryKey: ["folder-contents", payload.parent_folder_id],
  });
  queryClient.invalidateQueries({ queryKey: ["folders"] });

  if (!payload.parent_folder_id) {
    queryClient.invalidateQueries({ queryKey: ["folder-contents", null] });
  }

  if (!isOwnEvent(event)) {
    toastStore.show(`Folder "${payload.folder_name}" was created`, "info");
  }
}

function handleFolderRenamed(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate folder tree and all folder contents
  queryClient.invalidateQueries({ queryKey: ["folders"] });
  queryClient.invalidateQueries({ queryKey: ["folder-contents"] });

  if (!isOwnEvent(event)) {
    toastStore.show(
      `Folder renamed from "${payload.old_name}" to "${payload.new_name}"`,
      "info",
    );
  }
}

function handleFolderMoved(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate both old and new parent folder contents and folder tree
  queryClient.invalidateQueries({
    queryKey: ["folder-contents", payload.old_parent_id],
  });
  queryClient.invalidateQueries({
    queryKey: ["folder-contents", payload.new_parent_id],
  });
  queryClient.invalidateQueries({ queryKey: ["folders"] });

  if (!payload.old_parent_id || !payload.new_parent_id) {
    queryClient.invalidateQueries({ queryKey: ["folder-contents", null] });
  }

  if (!isOwnEvent(event)) {
    toastStore.show(`Folder "${payload.folder_name}" was moved`, "info");
  }
}

function handleFolderDeleted(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate parent folder contents and folder tree
  queryClient.invalidateQueries({
    queryKey: ["folder-contents", payload.parent_folder_id],
  });
  queryClient.invalidateQueries({ queryKey: ["folders"] });

  if (!payload.parent_folder_id) {
    queryClient.invalidateQueries({ queryKey: ["folder-contents", null] });
  }

  if (!isOwnEvent(event)) {
    toastStore.show(`Folder "${payload.folder_name}" was deleted`, "info");
  }
}

// Share Event Handlers
function handleShareCreated(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate shares list and file details
  queryClient.invalidateQueries({ queryKey: ["user-shares"] });
  queryClient.invalidateQueries({ queryKey: ["file", payload.file_id] });

  if (!isOwnEvent(event)) {
    toastStore.show(`Share created for "${payload.file_name}"`, "info");
  }
}

function handleShareRevoked(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate shares list and file details
  queryClient.invalidateQueries({ queryKey: ["user-shares"] });
  queryClient.invalidateQueries({ queryKey: ["file", payload.file_id] });

  if (!isOwnEvent(event)) {
    toastStore.show("Share was revoked", "info");
  }
}

function handleShareUpdated(event: WebSocketEvent): void {
  const payload = event.payload || (event as any);

  // Invalidate shares list and file details
  queryClient.invalidateQueries({ queryKey: ["user-shares"] });
  queryClient.invalidateQueries({ queryKey: ["file", payload.file_id] });

  if (!isOwnEvent(event)) {
    toastStore.show("Share was updated", "info");
  }
}

function handleReplicationStateChanged(event: WebSocketEvent): void {
  const fileId = event.file_id;
  const fileVersionId = event.file_version_id;
  const replicationState = event.replication_state;

  if (!fileId || !fileVersionId || !replicationState || !event.updated_at) {
    console.warn(
      "[WebSocket Manager] ReplicationStateChanged missing required fields",
      event,
    );
    return;
  }

  const status: ReplicationStatus = {
    fileId,
    fileVersionId,
    replicationState,
    jobStatus: event.job_status ?? null,
    attemptCount: event.attempt_count ?? 0,
    nextAttemptAt: event.next_attempt_at ?? null,
    lastError: event.last_error ?? null,
    updatedAt: event.updated_at,
  };

  replicationStore.upsert(status);
  queryClient.invalidateQueries({ queryKey: ["file", fileId] });

  if (!isOwnOrSystemEvent(event)) {
    return;
  }

  if (replicationState === "fully_replicated") {
    toastStore.show("File replication completed", "success");
  } else if (replicationState === "degraded") {
    toastStore.show("Replication delayed, retrying in background", "info");
  } else if (replicationState === "failed") {
    toastStore.show("Replication failed for a file version", "error");
  }
}
