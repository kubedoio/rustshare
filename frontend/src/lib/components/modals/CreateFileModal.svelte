<script lang="ts">
	import { FileText, File, PenTool, FileType } from 'lucide-svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import FolderTreePicker from './FolderTreePicker.svelte';

	type CreateFileType = 'txt' | 'md' | 'excalidraw' | 'odt';

	interface Props {
		open?: boolean;
		loading?: boolean;
		currentFolderId?: string | null;
		onClose?: () => void;
		onConfirm?: (payload: {
			targetFolderId: string | null;
			fileType: CreateFileType;
			fileName: string;
		}) => void;
	}

	let {
		open = false,
		loading = false,
		currentFolderId = null,
		onClose = () => {},
		onConfirm = () => {}
	}: Props = $props();

	let selectedFolderId: string | null = $state(null);
	let selectedType: CreateFileType = $state('txt');
	let fileName = $state('');
	let error = $state('');

	const fileTypes: {
		type: CreateFileType;
		label: string;
		icon: any;
		color: string;
		extension: string;
	}[] = [
		{ type: 'txt', label: 'Text', icon: FileText, color: 'text-gray-500', extension: '.txt' },
		{ type: 'md', label: 'Markdown', icon: FileText, color: 'text-blue-500', extension: '.md' },
		{
			type: 'excalidraw',
			label: 'Excalidraw',
			icon: PenTool,
			color: 'text-primary',
			extension: '.excalidraw'
		},
		{ type: 'odt', label: 'Document', icon: FileType, color: 'text-orange-500', extension: '.odt' }
	];

	const selectedExtension = $derived(
		fileTypes.find((t) => t.type === selectedType)?.extension || '.txt'
	);

	function handleSubmit() {
		error = '';

		const trimmedName = fileName.trim();
		if (!trimmedName) {
			error = 'Filename is required';
			return;
		}

		// Add extension if not present
		let finalName = trimmedName;
		if (!trimmedName.toLowerCase().endsWith(selectedExtension)) {
			finalName = trimmedName + selectedExtension;
		}

		onConfirm({
			targetFolderId: selectedFolderId,
			fileType: selectedType,
			fileName: finalName
		});
	}

	function handleClose() {
		error = '';
		fileName = '';
		selectedFolderId = currentFolderId;
		selectedType = 'txt';
		onClose();
	}

	// Reset when opened
	$effect(() => {
		if (open) {
			selectedFolderId = currentFolderId;
			fileName = '';
			error = '';
		}
	});
</script>

<ModalBase {open} title="Create New File" onClose={handleClose} class="flex max-h-[90vh] flex-col">
	<p class="mb-4 text-sm text-base-content/60">Choose location and file type</p>

	<!-- Location Section -->
	<div class="mb-5">
		<label class="mb-2 block text-sm font-medium text-base-content/80">
			Location
			<FolderTreePicker
				{selectedFolderId}
				{currentFolderId}
				onSelect={(id) => (selectedFolderId = id)}
			/>
		</label>
	</div>

	<!-- File Type Section -->
	<div class="mb-5">
		<label class="mb-2 block text-sm font-medium text-base-content/80">
			File Type
			<div class="grid grid-cols-2 gap-2">
				{#each fileTypes as ft}
					<button
						type="button"
						class="flex items-center gap-2 rounded-lg border p-3 text-left transition-all
            {selectedType === ft.type
							? 'border-brand-500 bg-brand-500/10'
							: 'border-base-300 hover:border-brand-500/30 hover:bg-base-200/50'}"
						onclick={() => (selectedType = ft.type)}
					>
						<ft.icon size={18} class={ft.color} />
						<span class="text-sm font-medium">{ft.label}</span>
					</button>
				{/each}
			</div>
		</label>
	</div>

	<!-- Filename Section -->
	<div>
		<label class="mb-2 block text-sm font-medium text-base-content/80" for="filename"
			>Filename</label
		>
		<input
			id="filename"
			type="text"
			class="input-bordered input w-full"
			class:input-error={error}
			placeholder="Enter filename"
			bind:value={fileName}
			disabled={loading}
			onkeydown={(e) => e.key === 'Enter' && handleSubmit()}
		/>
		{#if error}
			<p class="mt-1 text-sm text-error">{error}</p>
		{/if}
		<p class="mt-1 text-xs text-base-content/50">
			Extension {selectedExtension} will be added automatically
		</p>
	</div>

	<!-- Actions -->
	<div class="mt-6 flex shrink-0 justify-end gap-3">
		<button
			type="button"
			class="rounded-lg px-4 py-2 text-sm font-medium text-base-content/70 transition-colors hover:bg-base-200 hover:text-base-content"
			onclick={handleClose}
			disabled={loading}
		>
			Cancel
		</button>
		<button
			type="button"
			class="flex items-center gap-2 rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-600 disabled:opacity-50"
			onclick={handleSubmit}
			disabled={loading}
		>
			{#if loading}
				<span class="loading loading-sm loading-spinner"></span>
			{/if}
			Create
		</button>
	</div>
</ModalBase>
