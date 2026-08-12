<script lang="ts">
	import { onMount } from 'svelte';
	import { listAllFiles } from '$lib/api/files';
	import type { File } from '$lib/api/types';
	import { apiClient } from '$lib/api/client';
	import type { NostrTag } from '$lib/chat/nostr';

	interface Props {
		onSelect: (tag: NostrTag) => void;
	}

	let { onSelect }: Props = $props();

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

<button type="button" class="btn btn-sm" onclick={() => (open = true)}>Attach file</button>

{#if open}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
		onclick={() => (open = false)}
	>
		<div
			class="max-h-[70vh] w-96 overflow-y-auto rounded bg-base-100 p-4"
			onclick={(e) => e.stopPropagation()}
		>
			<h3 class="mb-2 font-semibold">Attach a file</h3>
			{#if loading}
				<div class="text-sm text-base-content/60">Loading files…</div>
			{:else if files.length === 0}
				<div class="text-sm text-base-content/60">No files yet.</div>
			{:else}
				<ul>
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
		</div>
	</div>
{/if}
