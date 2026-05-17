<script lang="ts">
	import {
		Plus,
		ChevronDown,
		File,
		Folder,
		FileText,
		Upload,
		CreditCard as Edit3,
		PenTool
	} from 'lucide-svelte';

	let {
		onAction,
		open = $bindable(false)
	}: {
		onAction: (action: string) => void;
		open?: boolean;
	} = $props();

	function handleAction(action: string) {
		open = false;
		onAction(action);
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			open = false;
		}
	}
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="relative">
	<button
		type="button"
		class="flex items-center gap-2 rounded-xl bg-brand-500 px-4 py-2 text-sm font-bold text-white shadow-lg shadow-brand-500/30 transition-all hover:bg-brand-600 hover:shadow-brand-500/40 active:scale-95"
		on:click={() => (open = !open)}
		aria-expanded={open}
		aria-haspopup="menu"
	>
		<Plus size={18} />
		<span>New</span>
		<ChevronDown size={14} class="opacity-60" />
	</button>

	{#if open}
		<div
			role="menu"
			class="animate-in fade-in zoom-in absolute left-0 mt-2 w-56 origin-top-left rounded-2xl border border-base-300 bg-base-100 p-1 shadow-xl ring-1 ring-black/5 duration-100"
		>
			<button
				role="menuitem"
				class="flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-sm font-medium transition-colors hover:bg-base-200"
				on:click={() => handleAction('create-file-requested')}
			>
				<File size={16} class="text-brand-500" /> File
			</button>
			<button
				role="menuitem"
				class="flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-sm font-medium transition-colors hover:bg-base-200"
				on:click={() => handleAction('create-folder-requested')}
			>
				<Folder size={16} class="text-brand-500" /> Folder
			</button>
			<button
				role="menuitem"
				class="mt-1 flex w-full items-center gap-3 rounded-xl border-t border-base-200 px-4 py-2.5 pt-2.5 text-sm font-medium transition-colors hover:bg-base-200"
				on:click={() => handleAction('create-note-requested')}
			>
				<FileText size={16} class="text-brand-500" /> New note
			</button>
			<button
				role="menuitem"
				class="flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-sm font-medium transition-colors hover:bg-base-200"
				on:click={() => handleAction('upload-requested')}
			>
				<Upload size={16} class="text-brand-500" /> Upload
			</button>
			<button
				role="menuitem"
				class="mt-1 flex w-full items-center gap-3 rounded-xl border-t border-base-200 px-4 py-2.5 pt-2.5 text-sm font-medium transition-colors hover:bg-base-200"
				on:click={() => handleAction('edit-file-requested')}
			>
				<Edit3 size={16} class="text-brand-500" /> Edit
			</button>
			<button
				role="menuitem"
				class="flex w-full items-center gap-3 rounded-xl px-4 py-2.5 text-sm font-medium transition-colors hover:bg-base-200"
				on:click={() => handleAction('create-canvas-requested')}
			>
				<PenTool size={16} class="text-brand-500" /> Sign
			</button>
		</div>
	{/if}
</div>
