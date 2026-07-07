<script lang="ts">
	import { createMutation, createQuery } from '$lib/query-compat';
	import { getVaultFileContent, saveVaultFileContent } from '$lib/api/vaults';
	import { queryClient } from '$lib/query-client';
	import type { VaultManifestEntry, VaultWritePolicy } from '$lib/api/types';
	import { isEditableVaultFile, isEditableVaultPolicy } from '$lib/utils/vault';
	import { Save, CircleAlert, Check, Loader, RotateCcw } from 'lucide-svelte';

	interface Props {
		vaultId: string;
		policy: VaultWritePolicy;
		file: VaultManifestEntry | null;
	}

	let { vaultId, policy, file }: Props = $props();

	let localContent = $state('');
	// Revision the user is editing against; sent as expected_revision.
	let editBaseRev = $state<number | null>(null);
	// Latest known server revision, updated by refetches without resetting the editor.
	let currentServerRev = $state<number | null>(null);
	let saveError = $state<string | null>(null);
	let saveSuccess = $state(false);
	let successTimeout = $state<ReturnType<typeof setTimeout> | null>(null);
	let loadedContent = $state<string | null>(null);
	let loadedPath = $state<string | null>(null);
	let dirty = $derived(file !== null && localContent !== (loadedContent ?? ''));
	let hasConflict = $derived(
		editBaseRev !== null && currentServerRev !== null && editBaseRev !== currentServerRev
	);

	const contentQuery = $derived(
		createQuery({
			queryKey: ['vault-file-content', vaultId, file?.path],
			queryFn: () => (file ? getVaultFileContent(vaultId, file.path) : Promise.reject('no file')),
			enabled: !!file && isEditableVaultFile(file)
		})
	);

	// Reset editor state when the selected file changes so we never save the
	// previous file's content/revision against a newly selected path.
	$effect(() => {
		const selectedPath = file?.path ?? null;
		if (selectedPath !== loadedPath) {
			localContent = '';
			loadedContent = null;
			editBaseRev = null;
			currentServerRev = null;
			loadedPath = selectedPath;
			saveError = null;
			saveSuccess = false;
		}
	});

	// Hydrate the editor from the query result. On a fresh file load we copy
	// server content into the editor. On refetch we only update the latest
	// known server revision so dirty local edits are preserved. We never
	// downgrade currentServerRev from cached data to avoid false conflicts
	// after a successful save.
	$effect(() => {
		const data = $contentQuery.data;
		if (data && file && data.path === file.path && file.path === loadedPath) {
			if (editBaseRev === null) {
				localContent = data.content;
				loadedContent = data.content;
				editBaseRev = data.server_rev;
				currentServerRev = data.server_rev;
			} else {
				currentServerRev =
					currentServerRev === null ? data.server_rev : Math.max(currentServerRev, data.server_rev);
			}
			saveError = null;
			saveSuccess = false;
		}
	});

	const saveMutation = createMutation({
		mutationFn: () => {
			if (!file || editBaseRev === null) throw new Error('No file loaded');
			if (hasConflict) throw new Error('File changed since opened');
			return saveVaultFileContent(vaultId, file.path, {
				content: localContent,
				expected_revision: editBaseRev
			});
		},
		onSuccess: (data) => {
			editBaseRev = data.server_rev;
			currentServerRev = data.server_rev;
			loadedContent = localContent;
			saveSuccess = true;
			saveError = null;
			queryClient.invalidateQueries({ queryKey: ['vault-manifest', vaultId] });
			if (successTimeout) clearTimeout(successTimeout);
			successTimeout = setTimeout(() => (saveSuccess = false), 3000);
		},
		onError: (err: { status?: number; message?: string }) => {
			if (err.status === 409) {
				saveError = 'This file was changed elsewhere. Copy your changes, reload, and try again.';
			} else if (err.status === 403) {
				saveError = 'You do not have permission to edit this file.';
			} else {
				saveError = err.message || 'Failed to save. Please try again.';
			}
		}
	});

	function reloadFromServer() {
		editBaseRev = null;
		loadedContent = null;
		queryClient.invalidateQueries({ queryKey: ['vault-file-content', vaultId, file?.path] });
	}

	$effect(() => {
		return () => {
			if (successTimeout) clearTimeout(successTimeout);
		};
	});

	const canEdit = $derived(
		file !== null && isEditableVaultFile(file) && isEditableVaultPolicy(policy)
	);
	const canSave = $derived(
		canEdit && dirty && editBaseRev !== null && !hasConflict && !$saveMutation.isPending
	);

	function handleKeyDown(event: KeyboardEvent) {
		if ((event.ctrlKey || event.metaKey) && event.key === 's') {
			event.preventDefault();
			if (canSave) $saveMutation.mutate().catch(() => {});
		}
	}
</script>

{#if !file}
	<div class="rounded-[1.25rem] border border-dashed border-base-300 bg-base-100 p-8 text-center">
		<p class="text-base-content/60">Select a file from the manifest to view or edit.</p>
	</div>
{:else if !isEditableVaultFile(file)}
	<div class="rounded-[1.25rem] border border-base-300/70 bg-base-100 p-6">
		<h3 class="font-display text-lg">{file.path}</h3>
		<p class="mt-2 text-sm text-base-content/60">
			This file type cannot be edited in the WebUI. Download it to edit locally.
		</p>
	</div>
{:else}
	<div class="space-y-3">
		<div class="flex items-center justify-between">
			<div>
				<h3 class="font-display text-lg">{file.path}</h3>
				<p class="text-xs text-base-content/50">
					rev {currentServerRev ?? file.server_rev}
					{#if !isEditableVaultPolicy(policy)}
						<span class="ml-2 rounded-full bg-warning/10 px-2 py-0.5 text-warning"
							>read-only vault</span
						>
					{/if}
					{#if hasConflict}
						<span class="ml-2 rounded-full bg-error/10 px-2 py-0.5 text-error">stale</span>
					{/if}
				</p>
			</div>
			{#if canEdit}
				<button
					class="btn btn-primary btn-sm rounded-xl"
					disabled={!canSave}
					onclick={() => $saveMutation.mutate().catch(() => {})}
				>
					{#if $saveMutation.isPending}
						<Loader class="h-4 w-4 animate-spin" />
					{:else if saveSuccess}
						<Check class="h-4 w-4" />
					{:else}
						<Save class="h-4 w-4" />
					{/if}
					<span>Save</span>
				</button>
			{/if}
		</div>

		{#if saveError}
			<div
				class="flex items-start gap-2 rounded-xl border border-error/20 bg-error/10 p-3 text-sm text-error"
			>
				<CircleAlert class="mt-0.5 h-4 w-4 shrink-0" />
				<span>{saveError}</span>
			</div>
		{/if}

		{#if hasConflict}
			<div
				class="flex items-start justify-between gap-3 rounded-xl border border-warning/20 bg-warning/10 p-3 text-sm text-warning"
			>
				<div class="flex items-start gap-2">
					<CircleAlert class="mt-0.5 h-4 w-4 shrink-0" />
					<span>This file changed since you opened it. Reload to see the latest version.</span>
				</div>
				<button class="btn btn-warning btn-xs rounded-lg" onclick={reloadFromServer}>
					<RotateCcw class="h-3 w-3" />
					<span>Reload</span>
				</button>
			</div>
		{/if}

		{#if canEdit}
			<textarea
				class="textarea textarea-bordered min-h-[24rem] w-full rounded-2xl font-mono text-sm"
				bind:value={localContent}
				onkeydown={handleKeyDown}
				disabled={$contentQuery.isLoading || $saveMutation.isPending}></textarea>
		{:else}
			<pre
				class="min-h-[24rem] overflow-auto rounded-2xl border border-base-300/70 bg-base-200/50 p-4 font-mono text-sm whitespace-pre-wrap">{$contentQuery
					.data?.content ?? ''}</pre>
		{/if}
	</div>
{/if}
