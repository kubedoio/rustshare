<script lang="ts">
	import { onMount } from 'svelte';
	import { listAllFiles } from '$lib/api/files';
	import type { File } from '$lib/api/types';
	import { apiClient } from '$lib/api/client';
	import type { NostrTag } from '$lib/chat/nostr';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import { Paperclip } from 'lucide-svelte';

	interface Props {
		onSelect: (tag: NostrTag) => void;
		iconOnly?: boolean;
	}

	let { onSelect, iconOnly = false }: Props = $props();

	let open = $state(false);
	let files = $state<File[]>([]);
	let loading = $state(false);
	let error = $state('');

	async function loadFiles(): Promise<void> {
		loading = true;
		error = '';
		try {
			files = await listAllFiles();
		} catch {
			error = 'Could not list files.';
		} finally {
			loading = false;
		}
	}

	onMount(loadFiles);

	async function pick(file: File): Promise<void> {
		try {
			const response = await apiClient.post<{ buzz_tag: NostrTag }>(
				'/applications/chat/attachments/prepare',
				{
					resource: {
						application: 'io.elembra.files',
						resourceType: 'file',
						resourceId: file.id
					}
				}
			);
			onSelect(response.buzz_tag);
			open = false;
		} catch (err) {
			error = err instanceof Error ? err.message : 'Attachment unavailable.';
		}
	}
</script>

{#if iconOnly}
	<button
		type="button"
		class="btn btn-ghost btn-xs h-8 w-8 rounded-lg p-0"
		aria-label="Attach file"
		title="Attach file"
		onclick={() => (open = true)}
	>
		<Paperclip size={16} class="text-base-content/60" />
	</button>
{:else}
	<button
		type="button"
		class="btn btn-sm inline-flex items-center gap-1.5"
		aria-label="Attach file"
		onclick={() => (open = true)}
	>
		<Paperclip size={16} />
		Attach file
	</button>
{/if}

<ModalBase {open} title="Attach a file" onClose={() => (open = false)} showCloseButton={false}>
	{#if loading}
		<div class="text-sm text-base-content/60">Loading files…</div>
	{:else if files.length === 0}
		<div class="text-sm text-base-content/60">No files yet.</div>
	{:else}
		<ul class="max-h-[50vh] overflow-y-auto">
			{#each files as file (file.id)}
				<li>
					<button
						type="button"
						class="w-full rounded px-2 py-1 text-left text-sm hover:bg-base-200"
						onclick={() => pick(file)}
					>
						{file.name}
					</button>
				</li>
			{/each}
		</ul>
	{/if}
	{#if error}<p class="mt-2 text-sm text-error">{error}</p>{/if}
	<button type="button" class="btn btn-sm mt-2" onclick={() => (open = false)}>Cancel</button>
</ModalBase>
