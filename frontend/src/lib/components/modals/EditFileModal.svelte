<script lang="ts">
	import { FileText, File, PenTool, Search, X } from 'lucide-svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import type { File as FileType } from '$lib/api/types';

	interface Props {
		open?: boolean;
		files?: FileType[];
		onClose?: () => void;
		onSelect?: (payload: { file: FileType }) => void;
	}

	let { open = false, files = [], onClose = () => {}, onSelect = () => {} }: Props = $props();

	let searchQuery = $state('');

	function getFileIcon(fileName: string) {
		const lower = fileName.toLowerCase();
		if (lower.endsWith('.md')) return { icon: FileText, color: 'text-blue-500', label: 'Markdown' };
		if (lower.endsWith('.excalidraw'))
			return { icon: PenTool, color: 'text-purple-500', label: 'Excalidraw' };
		return { icon: File, color: 'text-gray-500', label: 'Text' };
	}

	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
	}

	let filteredFiles = $derived(
		searchQuery.trim()
			? files.filter((f) => f.name.toLowerCase().includes(searchQuery.toLowerCase()))
			: files
	);

	function handleSelect(file: FileType) {
		onSelect({ file });
	}

	function handleClose() {
		searchQuery = '';
		onClose();
	}
</script>

<ModalBase
	{open}
	title="Select File to Edit"
	onClose={handleClose}
	class="flex max-h-[80vh] max-w-lg flex-col"
>
	<p class="mb-4 text-sm text-base-content/60">Choose a text, markdown, or excalidraw file</p>

	<!-- Search -->
	<div class="mb-3 shrink-0">
		<div class="relative">
			<Search size={16} class="absolute top-1/2 left-3 -translate-y-1/2 text-base-content/40" />
			<input
				type="text"
				class="input-bordered input w-full pl-10"
				placeholder="Search files..."
				bind:value={searchQuery}
			/>
			{#if searchQuery}
				<button
					type="button"
					class="absolute top-1/2 right-3 -translate-y-1/2 text-base-content/40 hover:text-base-content"
					onclick={() => (searchQuery = '')}
				>
					<X size={14} />
				</button>
			{/if}
		</div>
	</div>

	<!-- File List -->
	<div class="-mx-5 -mb-5 flex-1 overflow-y-auto px-2 py-2">
		{#if filteredFiles.length === 0}
			<div class="py-8 text-center">
				<p class="text-sm text-base-content/60">
					{searchQuery ? 'No files match your search' : 'No editable files in this folder'}
				</p>
				<p class="mt-1 text-xs text-base-content/40">Supported: .txt, .md, .excalidraw</p>
			</div>
		{:else}
			<div class="space-y-1">
				{#each filteredFiles as file (file.id)}
					{@const iconInfo = getFileIcon(file.name)}
					<button
						type="button"
						class="flex w-full items-center gap-3 rounded-lg px-3 py-3 text-left transition-colors hover:bg-base-200"
						onclick={() => handleSelect(file)}
					>
						<iconInfo.icon size={20} class={iconInfo.color} />
						<div class="min-w-0 flex-1">
							<p class="truncate text-sm font-medium text-base-content">{file.name}</p>
							<p class="text-xs text-base-content/50">Modified {formatDate(file.modified_at)}</p>
						</div>
						<span
							class="shrink-0 rounded-full bg-base-200 px-2 py-0.5 text-xs text-base-content/60"
						>
							{iconInfo.label}
						</span>
					</button>
				{/each}
			</div>
		{/if}
	</div>

	<!-- Actions -->
	<div class="mt-4 flex shrink-0 justify-end">
		<button
			type="button"
			class="rounded-lg px-4 py-2 text-sm font-medium text-base-content/70 transition-colors hover:bg-base-200 hover:text-base-content"
			onclick={handleClose}
		>
			Cancel
		</button>
	</div>
</ModalBase>
