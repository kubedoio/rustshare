<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { File } from '$lib/api/types';
	import { formatFileSize } from '$lib/utils/format';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';

	let {
		open = false,
		file = null,
		title = 'Edit File',
		isLoading = false,
		isSaving = false,
		error = null,
		saveMode = 'new_version',
		hasChanges = false
	}: {
		open?: boolean;
		file?: File | null;
		title?: string;
		isLoading?: boolean;
		isSaving?: boolean;
		error?: string | null;
		saveMode?: 'overwrite' | 'new_version';
		hasChanges?: boolean;
	} = $props();

	type EditorEvents = {
		close: void;
		save: { saveMode: 'overwrite' | 'new_version'; changeDescription?: string };
	};
	const dispatch = createEventDispatcher<EditorEvents>();

	let changeDescription = $state('');
	let showSaveOptions = $state(false);
	let showConfirmModal = $state(false);

	function handleClose() {
		if (hasChanges) {
			showConfirmModal = true;
			return;
		}
		dispatch('close');
	}

	function handleConfirmClose() {
		showConfirmModal = false;
		dispatch('close');
	}

	function handleSave(forceMode?: 'overwrite' | 'new_version') {
		const mode = forceMode || saveMode;
		dispatch('save', {
			saveMode: mode,
			changeDescription: mode === 'new_version' ? changeDescription : undefined
		});
		showSaveOptions = false;
	}

	function handleKeydown(event: KeyboardEvent) {
		// Ctrl+S / Cmd+S to save
		if ((event.ctrlKey || event.metaKey) && event.key === 's') {
			event.preventDefault();
			if (!isSaving && hasChanges) {
				handleSave();
			}
		}
		// Escape to close
		if (event.key === 'Escape' && !isSaving) {
			handleClose();
		}
	}

	let canSave = $derived(hasChanges && !isSaving);
</script>

<svelte:window onkeydown={handleKeydown} />

<dialog class="modal" class:modal-open={open} {open}>
	<div class="modal-box flex h-[90vh] max-w-7xl flex-col p-0">
		<!-- Header -->
		<div class="border-b border-base-300 px-6 py-4">
			<div class="flex items-center justify-between">
				<div class="min-w-0 flex-1">
					<h3 class="truncate text-lg font-bold">{file?.name || title}</h3>
					{#if file}
						<p class="text-sm text-base-content/60">
							{formatFileSize(file.size)} • {file.mime_type}
							{#if hasChanges}
								<span class="ml-2 text-warning">(unsaved changes)</span>
							{/if}
						</p>
					{/if}
				</div>

				<button
					type="button"
					class="btn btn-circle btn-ghost btn-sm"
					aria-label="Close editor"
					onclick={handleClose}
					disabled={isSaving}
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						stroke-width="1.5"
						stroke="currentColor"
						class="h-6 w-6"
					>
						<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
					</svg>
				</button>
			</div>

			{#if error}
				<div class="mt-3 alert text-sm alert-error">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						class="h-5 w-5 shrink-0"
						fill="none"
						viewBox="0 0 24 24"
						stroke="currentColor"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="2"
							d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"
						/>
					</svg>
					<span>{error}</span>
				</div>
			{/if}
		</div>

		<!-- Content Area -->
		<div class="relative flex-1 overflow-hidden">
			<slot />

			{#if isLoading}
				<div class="absolute inset-0 z-20 flex flex-col items-center justify-center bg-base-100/80">
					<span class="loading loading-lg loading-spinner text-primary"></span>
					<p class="mt-4 text-sm text-base-content/60">Loading file content...</p>
				</div>
			{/if}
		</div>

		<!-- Footer -->
		<div class="border-t border-base-300 px-6 py-4">
			<div class="flex items-center justify-between gap-4">
				<!-- Save Mode Selection -->
				<div class="flex items-center gap-4">
					<label class="flex cursor-pointer items-center gap-2">
						<input
							type="radio"
							name="saveMode"
							value="new_version"
							bind:group={saveMode}
							class="radio radio-sm radio-primary"
							disabled={isSaving}
						/>
						<span class="text-sm">Create new version</span>
					</label>
					<label class="flex cursor-pointer items-center gap-2">
						<input
							type="radio"
							name="saveMode"
							value="overwrite"
							bind:group={saveMode}
							class="radio radio-sm radio-primary"
							disabled={isSaving}
						/>
						<span class="text-sm">Overwrite current</span>
					</label>
				</div>

				<!-- Change Description (only for new version) -->
				{#if saveMode === 'new_version'}
					<div class="max-w-md flex-1">
						<input
							type="text"
							placeholder="Change description (optional)"
							bind:value={changeDescription}
							class="input-bordered input input-sm w-full"
							disabled={isSaving}
						/>
					</div>
				{/if}

				<!-- Action Buttons -->
				<div class="flex items-center gap-2">
					<button
						type="button"
						class="btn btn-ghost btn-sm"
						onclick={handleClose}
						disabled={isSaving}
					>
						Cancel
					</button>

					<div class="dropdown dropdown-end dropdown-top">
						<button
							type="button"
							class="btn btn-sm btn-primary"
							disabled={!canSave}
							onclick={() => handleSave()}
						>
							{#if isSaving}
								<span class="loading loading-xs loading-spinner"></span>
								Saving...
							{:else}
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="h-4 w-4"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M16.5 3.75V16.5L12 14.25 7.5 16.5V3.75m9 0H18A2.25 2.25 0 0120.25 6v12A2.25 2.25 0 0118 20.25H6A2.25 2.25 0 013.75 18V6A2.25 2.25 0 016 3.75h1.5m9 0h-9"
									/>
								</svg>
								Save
							{/if}
						</button>
					</div>
				</div>
			</div>

			<!-- Keyboard shortcuts hint -->
			<div class="mt-2 text-right text-xs text-base-content/40">
				Keyboard shortcuts: <kbd class="kbd kbd-sm">Ctrl+S</kbd> to save,
				<kbd class="kbd kbd-sm">Esc</kbd> to close
			</div>
		</div>
	</div>

	<form method="dialog" class="modal-backdrop">
		<button type="button" onclick={handleClose}>close</button>
	</form>
</dialog>

<ConfirmModal
	open={showConfirmModal}
	title="Unsaved Changes"
	message="You have unsaved changes. Are you sure you want to close?"
	confirmLabel="Close"
	cancelLabel="Cancel"
	danger={true}
	onConfirm={handleConfirmClose}
	onCancel={() => (showConfirmModal = false)}
/>
