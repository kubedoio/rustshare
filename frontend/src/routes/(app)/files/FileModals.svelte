<script lang="ts">
	import type { File, Folder } from '$lib/api/types';

	import RenameModal from '$lib/components/modals/RenameModal.svelte';
	import DeleteConfirmation from '$lib/components/modals/DeleteConfirmation.svelte';
	import MoveModal from '$lib/components/modals/MoveModal.svelte';
	import ShareModal from '$lib/components/modals/ShareModal.svelte';
	import CreateFolderModal from '$lib/components/modals/CreateFolderModal.svelte';
	import VersionHistoryModal from '$lib/components/modals/VersionHistoryModal.svelte';
	import FilePreviewModal from '$lib/components/modals/FilePreviewModal.svelte';
	import ReplaceFileModal from '$lib/components/modals/ReplaceFileModal.svelte';
	import CreateFileModal from '$lib/components/modals/CreateFileModal.svelte';
	import UploadTargetModal from '$lib/components/modals/UploadTargetModal.svelte';
	import EditFileModal from '$lib/components/modals/EditFileModal.svelte';

	interface Props {
		currentFolderId: string | null;
		moveCurrentFolderId: string | null;

		// Rename
		showRenameModal: boolean;
		renameTarget: File | Folder | null;
		renameType: 'file' | 'folder';
		renameLoading: boolean;
		onRenameClose: () => void;
		onRenameConfirm: (event: { newName: string }) => void;

		// Delete
		showDeleteModal: boolean;
		deleteTarget: File | Folder | null;
		deleteType: 'file' | 'folder';
		deleteLoading: boolean;
		onDeleteClose: () => void;
		onDeleteConfirm: () => void;

		// Move
		showMoveModal: boolean;
		moveTarget: File | Folder | null;
		moveType: 'file' | 'folder';
		moveLoading: boolean;
		bulkMoveFileIds: string[];
		bulkMoveLoading: boolean;
		onMoveClose: () => void;
		onMoveConfirm: (event: { targetFolderId: string | null }) => void;

		// Create Folder
		showCreateFolderModal: boolean;
		createFolderLoading: boolean;
		onCreateFolderClose: () => void;
		onCreateFolderConfirm: (event: { name: string; parentFolderId: string | null }) => void;

		// Create File
		showCreateFileModal: boolean;
		createFileLoading: boolean;
		onCreateFileClose: () => void;
		onCreateFileConfirm: (event: { targetFolderId: string | null; fileType: string; fileName: string }) => void;

		// Upload Target
		showUploadTargetModal: boolean;
		onUploadTargetClose: () => void;
		onUploadTargetConfirm: (event: { targetFolderId: string | null }) => void;

		// Edit File
		showEditFileModal: boolean;
		editableFilesForModal: File[];
		onEditFileClose: () => void;
		onEditFileSelect: (event: { file: File }) => void;

		// Share
		showShareModal: boolean;
		shareTarget: File | Folder | null;
		shareType: 'file' | 'folder';
		onShareClose: () => void;
		onShareNotification: (event: { message: string; type: 'success' | 'error' | 'info' }) => void;

		// Version History
		showVersionHistoryModal: boolean;
		versionHistoryTarget: File | null;
		onVersionHistoryClose: () => void;
		onVersionRestored: () => void;

		// File Preview
		showFilePreviewModal: boolean;
		previewTarget: File | null;
		onFilePreviewClose: () => void;
		onEditFile: (event: { file: File } | File) => void;

		// Replace File
		showReplaceFileModal: boolean;
		replaceFileTarget: File | null;
		onReplaceFileClose: () => void;
		onReplaceSuccess: () => void;
	}

	let {
		currentFolderId,
		moveCurrentFolderId,
		showRenameModal,
		renameTarget,
		renameType,
		renameLoading,
		onRenameClose,
		onRenameConfirm,
		showDeleteModal,
		deleteTarget,
		deleteType,
		deleteLoading,
		onDeleteClose,
		onDeleteConfirm,
		showMoveModal,
		moveTarget,
		moveType,
		moveLoading,
		bulkMoveFileIds,
		bulkMoveLoading,
		onMoveClose,
		onMoveConfirm,
		showCreateFolderModal,
		createFolderLoading,
		onCreateFolderClose,
		onCreateFolderConfirm,
		showCreateFileModal,
		createFileLoading,
		onCreateFileClose,
		onCreateFileConfirm,
		showUploadTargetModal,
		onUploadTargetClose,
		onUploadTargetConfirm,
		showEditFileModal,
		editableFilesForModal,
		onEditFileClose,
		onEditFileSelect,
		showShareModal,
		shareTarget,
		shareType,
		onShareClose,
		onShareNotification,
		showVersionHistoryModal,
		versionHistoryTarget,
		onVersionHistoryClose,
		onVersionRestored,
		showFilePreviewModal,
		previewTarget,
		onFilePreviewClose,
		onEditFile,
		showReplaceFileModal,
		replaceFileTarget,
		onReplaceFileClose,
		onReplaceSuccess
	}: Props = $props();
</script>

{#if showRenameModal}
	<RenameModal
		open={showRenameModal}
		loading={renameLoading}
		itemName={renameTarget?.name || ''}
		itemType={renameType}
		onClose={onRenameClose}
		onConfirm={onRenameConfirm}
	/>
{/if}

{#if showDeleteModal}
	<DeleteConfirmation
		open={showDeleteModal}
		loading={deleteLoading}
		itemName={deleteTarget?.name || ''}
		itemType={deleteType}
		onClose={onDeleteClose}
		onConfirm={onDeleteConfirm}
	/>
{/if}

{#if showMoveModal}
	<MoveModal
		open={showMoveModal}
		loading={moveLoading || bulkMoveLoading}
		itemName={bulkMoveFileIds.length > 0 ? `${bulkMoveFileIds.length} selected file${bulkMoveFileIds.length === 1 ? '' : 's'}` : moveTarget?.name || ''}
		itemType={moveType}
		itemId={bulkMoveFileIds.length > 0 ? null : moveTarget?.id || null}
		currentFolderId={bulkMoveFileIds.length > 0 ? currentFolderId : moveCurrentFolderId}
		onClose={onMoveClose}
		onConfirm={onMoveConfirm}
	/>
{/if}

{#if showCreateFolderModal}
	<CreateFolderModal
		open={showCreateFolderModal}
		loading={createFolderLoading}
		{currentFolderId}
		onClose={onCreateFolderClose}
		onConfirm={onCreateFolderConfirm}
	/>
{/if}

{#if showCreateFileModal}
	<CreateFileModal
		open={showCreateFileModal}
		loading={createFileLoading}
		{currentFolderId}
		onClose={onCreateFileClose}
		onConfirm={onCreateFileConfirm}
	/>
{/if}

{#if showUploadTargetModal}
	<UploadTargetModal
		open={showUploadTargetModal}
		{currentFolderId}
		onClose={onUploadTargetClose}
		onConfirm={onUploadTargetConfirm}
	/>
{/if}

{#if showEditFileModal}
	<EditFileModal
		open={showEditFileModal}
		files={editableFilesForModal}
		onClose={onEditFileClose}
		onSelect={onEditFileSelect}
	/>
{/if}

{#if showShareModal}
	<ShareModal
		open={showShareModal}
		resourceId={shareTarget?.id || ''}
		resourceName={shareTarget?.name || ''}
		resourceType={shareType}
		onClose={onShareClose}
		onNotification={onShareNotification}
	/>
{/if}

{#if showVersionHistoryModal && versionHistoryTarget}
	<VersionHistoryModal
		open={showVersionHistoryModal}
		fileId={versionHistoryTarget.id}
		fileName={versionHistoryTarget.name}
		onClose={onVersionHistoryClose}
		onRestored={onVersionRestored}
	/>
{/if}

{#if showFilePreviewModal && previewTarget}
	<FilePreviewModal
		open={showFilePreviewModal}
		file={previewTarget}
		onClose={onFilePreviewClose}
		onEdit={onEditFile}
	/>
{/if}

{#if showReplaceFileModal && replaceFileTarget}
	<ReplaceFileModal
		open={showReplaceFileModal}
		file={replaceFileTarget}
		onClose={onReplaceFileClose}
		onSuccess={onReplaceSuccess}
	/>
{/if}
