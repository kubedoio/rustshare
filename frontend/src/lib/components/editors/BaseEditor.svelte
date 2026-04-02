<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { File } from '$lib/api/types';
	import { formatFileSize } from '$lib/utils/format';

	export let open = false;
	export let file: File | null = null;
	export let title = 'Edit File';
	export let isLoading = false;
	export let isSaving = false;
	export let error: string | null = null;
	export let saveMode: 'overwrite' | 'new_version' = 'overwrite';
	export let hasChanges = false;

	type EditorEvents = {
		close: void;
		save: { saveMode: 'overwrite' | 'new_version'; changeDescription?: string };
	};
	const dispatch = createEventDispatcher<EditorEvents>();

	let changeDescription = '';
	let showSaveOptions = false;

	function handleClose() {
		if (hasChanges) {
			if (!confirm('You have unsaved changes. Are you sure you want to close?')) {
				return;
			}
		}
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

	$: canSave = hasChanges && !isSaving;
</script>

<svelte:window on:keydown={handleKeydown} />

<dialog class="modal" class:modal-open={open}>
	<div class="modal-box max-w-7xl flex h-[90vh] flex-col p-0">
		<!-- Header -->
		<div class="border-b border-base-300 px-6 py-4">
			<div class="flex items-center justify-between">
				<div class="min-w-0 flex-1">
					<h3 class="font-bold text-lg truncate">{file?.name || title}</h3>
					{#if file}
						<p class="text-sm text-base-content/60">
							{formatFileSize(file.size)} • {file.mime_type}
							{#if hasChanges}
								<span class="text-warning ml-2">(unsaved changes)</span>
							{/if}
						</p>
					{/if}
				</div>

				<button
					type="button"
					class="btn btn-ghost btn-sm btn-circle"
					aria-label="Close editor"
					on:click={handleClose}
					disabled={isSaving}
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						stroke-width="1.5"
						stroke="currentColor"
						class="w-6 h-6"
					>
						<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
					</svg>
				</button>
			</div>

			{#if error}
				<div class="alert alert-error mt-3 text-sm">
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
		<div class="flex-1 overflow-hidden relative">
			{#if isLoading}
				<div class="absolute inset-0 flex flex-col items-center justify-center bg-base-100">
					<span class="loading loading-spinner loading-lg text-primary"></span>
					<p class="text-sm text-base-content/60 mt-4">Loading file content...</p>
				</div>
			{:else}
				<slot />
			{/if}
		</div>

		<!-- Footer -->
		<div class="border-t border-base-300 px-6 py-4">
			<div class="flex items-center justify-between gap-4">
				<!-- Save Mode Selection -->
				<div class="flex items-center gap-4">
					<label class="flex items-center gap-2 cursor-pointer">
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
					<label class="flex items-center gap-2 cursor-pointer">
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
				</div>

				<!-- Change Description (only for new version) -->
				{#if saveMode === 'new_version'}
					<div class="flex-1 max-w-md">
						<input
							type="text"
							placeholder="Change description (optional)"
							bind:value={changeDescription}
							class="input input-sm input-bordered w-full"
							disabled={isSaving}
						/>
					</div>
				{/if}

				<!-- Action Buttons -->
				<div class="flex items-center gap-2">
					<button
						type="button"
						class="btn btn-ghost btn-sm"
						on:click={handleClose}
						disabled={isSaving}
					>
						Cancel
					</button>

					<div class="dropdown dropdown-top dropdown-end">
						<button
							type="button"
							class="btn btn-primary btn-sm"
							disabled={!canSave}
							on:click={() => handleSave()}
						>
							{#if isSaving}
								<span class="loading loading-spinner loading-xs"></span>
								Saving...
							{:else}
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="w-4 h-4"
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
			<div class="text-xs text-base-content/40 mt-2 text-right">
				Keyboard shortcuts: <kbd class="kbd kbd-sm">Ctrl+S</kbd> to save,
				<kbd class="kbd kbd-sm">Esc</kbd> to close
			</div>
		</div>
	</div>

	<form method="dialog" class="modal-backdrop">
		<button type="button" on:click={handleClose}>close</button>
	</form>
</dialog>
