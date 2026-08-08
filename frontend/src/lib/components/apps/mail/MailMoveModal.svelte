<script lang="ts">
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import { Folder, Loader2 } from 'lucide-svelte';
	import type { MailFolder } from '$lib/api/mail';

	interface Props {
		open: boolean;
		folders: MailFolder[];
		currentFolder: string | null;
		isLoading?: boolean;
		onClose: () => void;
		onMove: (folderName: string) => void;
	}

	let { open, folders, currentFolder, isLoading = false, onClose, onMove }: Props = $props();
</script>

<ModalBase {open} title="Move message" {onClose}>
	<div class="flex flex-col gap-4">
		<p class="text-sm text-base-content/70">Choose a folder to move the message into.</p>

		<div class="max-h-80 overflow-y-auto rounded-lg border border-base-300/50 bg-base-200/30">
			{#if folders.length === 0}
				<p class="p-4 text-sm text-base-content/55">No folders available.</p>
			{:else}
				{#each folders as folder}
					<button
						type="button"
						class="flex w-full items-center gap-3 border-b border-base-300/30 px-4 py-3 text-left text-sm transition-colors last:border-b-0 {folder.name ===
						currentFolder
							? 'bg-base-300/30 text-base-content/50'
							: 'hover:bg-base-200/60'}"
						disabled={folder.name === currentFolder || isLoading}
						onclick={() => onMove(folder.name)}
					>
						<Folder size={16} />
						<span class="min-w-0 flex-1 truncate">{folder.display_name}</span>
						{#if folder.name === currentFolder}
							<span class="text-xs text-base-content/50">Current</span>
						{/if}
					</button>
				{/each}
			{/if}
		</div>

		{#if isLoading}
			<div class="flex items-center justify-center gap-2 text-sm text-base-content/60">
				<Loader2 size={16} class="animate-spin" />
				<span>Moving...</span>
			</div>
		{/if}

		<div class="flex justify-end gap-2">
			<button type="button" class="btn btn-ghost btn-sm" onclick={onClose}>Cancel</button>
		</div>
	</div>
</ModalBase>
