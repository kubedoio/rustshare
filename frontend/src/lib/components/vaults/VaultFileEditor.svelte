<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { createMutation, createQuery } from '$lib/query-compat';
	import { getVaultFileContent, saveVaultFileContent } from '$lib/api/vaults';
	import { queryClient } from '$lib/query-client';
	import RichMarkdownEditor from '$lib/editor/components/RichMarkdownEditor.svelte';
	import { splitFrontmatter, wrapFrontmatter } from '$lib/editor/adapter/frontmatter';
	import type { VaultManifestEntry, VaultWritePolicy } from '$lib/api/types';
	import { isEditableVaultFile, isEditableVaultPolicy } from '$lib/utils/vault';
	import { sha256Hex } from '$lib/utils/sha256';
	import { Save, CircleAlert, Check, Loader, RotateCcw, Copy, Download, X } from 'lucide-svelte';

	interface Props {
		vaultId: string;
		policy: VaultWritePolicy;
		file: VaultManifestEntry | null;
		/** Mirrors the internal dirty flag so the parent page can guard file switching. */
		dirty?: boolean;
	}

	let { vaultId, policy, file = $bindable(), dirty = $bindable(false) }: Props = $props();

	let localContent = $state('');
	// Revision the user is editing against; sent as expected_revision.
	let editBaseRev = $state<number | null>(null);
	// Latest known server revision, updated by refetches without resetting the editor.
	let currentServerRev = $state<number | null>(null);
	// The server rejected a save with an unstructured 409 (no current_rev): the
	// file was tombstoned and can never be saved again from this editor.
	let tombstoneConflict = $state(false);
	let saveError = $state<string | null>(null);
	let saveSuccess = $state(false);
	let successTimeout = $state<ReturnType<typeof setTimeout> | null>(null);
	let conflictCopied = $state(false);
	let copiedTimeout = $state<ReturnType<typeof setTimeout> | null>(null);
	let loadedContent = $state<string | null>(null);
	let loadedPath = $state<string | null>(null);
	let loadedVaultId = $state<string | null>(null);
	let editorRegion = $state<HTMLDivElement>();
	let richEditor = $state<RichMarkdownEditor>();
	let isDirty = $derived(file !== null && localContent !== (loadedContent ?? ''));
	let hasConflict = $derived(
		editBaseRev !== null && currentServerRev !== null && editBaseRev !== currentServerRev
	);

	// Expose the dirty flag to the parent so it can confirm before dropping
	// unsaved content when switching files.
	$effect(() => {
		dirty = isDirty;
	});

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
		if (selectedPath !== loadedPath || vaultId !== loadedVaultId) {
			localContent = '';
			loadedContent = null;
			editBaseRev = null;
			currentServerRev = null;
			loadedPath = selectedPath;
			loadedVaultId = vaultId;
			saveError = null;
			saveSuccess = false;
			conflictCopied = false;
			tombstoneConflict = false;
		}
	});

	// Hydrate the editor from the query result. On a fresh file load we copy
	// server content into the editor. On refetch we only update the latest
	// known server revision so dirty local edits are preserved. We never
	// downgrade currentServerRev from cached data to avoid false conflicts
	// after a successful save. When the refetched server content is identical
	// to the editor content, the "conflict" is only a revision mismatch, so we
	// silently adopt the server revision instead of alarming the user.
	$effect(() => {
		const data = $contentQuery.data;
		if (
			data &&
			file &&
			data.path === file.path &&
			file.path === loadedPath &&
			vaultId === loadedVaultId
		) {
			const isNewerOrSame = currentServerRev === null || data.server_rev >= currentServerRev;
			if ((editBaseRev === null || !isDirty) && isNewerOrSame) {
				localContent = data.content;
				loadedContent = data.content;
				editBaseRev = data.server_rev;
				currentServerRev = data.server_rev;
				saveError = null;
			} else if (
				editBaseRev !== null &&
				data.server_rev > editBaseRev &&
				data.content === localContent
			) {
				loadedContent = data.content;
				editBaseRev = data.server_rev;
				currentServerRev = data.server_rev;
				saveError = null;
			} else {
				currentServerRev =
					currentServerRev === null ? data.server_rev : Math.max(currentServerRev, data.server_rev);
			}
			saveSuccess = false;
		}
	});

	const saveMutation = createMutation({
		mutationFn: () => {
			if (!file || editBaseRev === null) throw new Error('No file loaded');
			if (hasConflict) throw new Error('File changed since opened');
			if (tombstoneConflict) throw new Error('File was deleted on the server');
			return saveVaultFileContent(vaultId, file.path, {
				content: localContent,
				expected_revision: editBaseRev
			});
		},
		onSuccess: (data) => {
			editBaseRev = data.server_rev;
			currentServerRev = data.server_rev;
			loadedContent = localContent;
			markSaved(data.path, data.server_rev);
		},
		onError: async (err: {
			status?: number;
			message?: string;
			current_rev?: number;
			server_sha256?: string;
		}) => {
			if (err.status === 409) {
				const currentRev = typeof err.current_rev === 'number' ? err.current_rev : null;
				// When the server already holds exactly what the user has in the
				// editor, the conflict is only a revision mismatch: silently adopt
				// the server revision instead of showing a conflict.
				if (currentRev !== null && typeof err.server_sha256 === 'string') {
					const localSha = await sha256Hex(localContent);
					if (localSha !== null && localSha === err.server_sha256.toLowerCase()) {
						editBaseRev = currentRev;
						currentServerRev = currentRev;
						loadedContent = localContent;
						markSaved(file?.path ?? '', currentRev);
						return;
					}
				}
				if (currentRev !== null) {
					currentServerRev =
						currentServerRev === null ? currentRev : Math.max(currentServerRev, currentRev);
					saveError = null;
					queryClient.invalidateQueries({ queryKey: ['vault-file-content', vaultId, file?.path] });
				} else {
					// Unstructured 409 (no current_rev): the file was tombstoned on the
					// server, so it can never be saved again and reloading would 404.
					// Keep the local text and show the tombstone recovery panel
					// (copy/download/close) instead of a reload action.
					saveError = null;
					tombstoneConflict = true;
					queryClient.invalidateQueries({ queryKey: ['vault-manifest', vaultId] });
				}
			} else if (err.status === 403) {
				saveError = 'You do not have permission to edit this file.';
			} else {
				saveError = err.message || 'Failed to save. Please try again.';
			}
		}
	});

	// Shared bookkeeping for a state where the server holds the editor content:
	// a successful save, or a 409 whose server_sha256 matches the editor content.
	function markSaved(path: string, serverRev: number) {
		saveSuccess = true;
		saveError = null;
		queryClient.setQueryData(['vault-file-content', vaultId, path], {
			path,
			content: localContent,
			server_rev: serverRev,
			content_type: $contentQuery.data?.content_type ?? 'text/markdown',
			size: new Blob([localContent]).size
		});
		queryClient.invalidateQueries({ queryKey: ['vault-manifest', vaultId] });
		if (successTimeout) clearTimeout(successTimeout);
		successTimeout = setTimeout(() => (saveSuccess = false), 3000);
	}

	function reloadFromServer() {
		editBaseRev = null;
		loadedContent = null;
		saveError = null;
		conflictCopied = false;
		queryClient.invalidateQueries({ queryKey: ['vault-file-content', vaultId, file?.path] });
	}

	function confirmReloadFromServer() {
		if (confirm('Discard your unsaved changes and reload the server version?')) {
			reloadFromServer();
		}
	}

	async function copyMyChanges() {
		syncMarkdown();
		try {
			await navigator.clipboard.writeText(localContent);
			conflictCopied = true;
			if (copiedTimeout) clearTimeout(copiedTimeout);
			copiedTimeout = setTimeout(() => (conflictCopied = false), 2000);
		} catch {
			saveError = 'Could not copy to the clipboard. Select the text and copy it manually.';
		}
	}

	function downloadMyVersion() {
		if (!file) return;
		syncMarkdown();
		const name = file.path.split('/').pop() || 'vault-file.md';
		const blob = new Blob([localContent], { type: 'text/markdown;charset=utf-8' });
		const url = URL.createObjectURL(blob);
		const anchor = document.createElement('a');
		anchor.href = url;
		anchor.download = name;
		anchor.click();
		URL.revokeObjectURL(url);
	}

	// Dismissing a tombstoned file deselects it; the file-switch reset effect
	// then clears the editor. The local text stays copyable/downloadable until
	// the user chooses to close.
	function closeFile() {
		file = null;
	}

	// Warn before losing unsaved changes on tab close/refresh or in-app navigation.
	function handleBeforeUnload(event: BeforeUnloadEvent) {
		syncMarkdown();
		if (isDirty) {
			event.preventDefault();
			event.returnValue = '';
		}
	}

	beforeNavigate((navigation) => {
		syncMarkdown();
		if (isDirty && !confirm('You have unsaved changes. Leave without saving?')) {
			navigation.cancel();
		}
	});

	onMount(() => {
		window.addEventListener('beforeunload', handleBeforeUnload);
	});

	onDestroy(() => {
		window.removeEventListener('beforeunload', handleBeforeUnload);
		if (copiedTimeout) clearTimeout(copiedTimeout);
	});

	$effect(() => {
		return () => {
			if (successTimeout) clearTimeout(successTimeout);
		};
	});

	const canEdit = $derived(
		file !== null && isEditableVaultFile(file) && isEditableVaultPolicy(policy)
	);
	const canSave = $derived(
		canEdit &&
			isDirty &&
			editBaseRev !== null &&
			!hasConflict &&
			!tombstoneConflict &&
			!$saveMutation.isPending
	);
	const isMarkdown = $derived(
		file?.path.toLowerCase().endsWith('.md') || file?.path.toLowerCase().endsWith('.markdown')
	);
	const editorDocument = $derived(splitFrontmatter(localContent));

	function syncMarkdown() {
		if (!isMarkdown || !richEditor) return;
		const markdown = richEditor.getMarkdown();
		localContent = editorDocument.hasFrontmatter
			? wrapFrontmatter(editorDocument.frontmatter, markdown)
			: markdown;
	}

	function save() {
		syncMarkdown();
		if (
			canEdit &&
			localContent !== (loadedContent ?? '') &&
			editBaseRev !== null &&
			!hasConflict &&
			!tombstoneConflict &&
			!$saveMutation.isPending
		) {
			$saveMutation.mutate().catch(() => {});
		}
	}

	function handleKeyDown(event: KeyboardEvent) {
		if (
			(event.ctrlKey || event.metaKey) &&
			event.key === 's' &&
			editorRegion?.contains(event.target as Node)
		) {
			event.preventDefault();
			save();
		}
	}
</script>

<svelte:window onkeydown={handleKeyDown} />

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
	<div
		class="space-y-3"
		bind:this={editorRegion}
		onfocusout={(event) => {
			if (!editorRegion?.contains(event.relatedTarget as Node | null)) syncMarkdown();
		}}
	>
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
				<button class="btn rounded-xl btn-primary btn-sm" disabled={!canSave} onclick={save}>
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
				class="space-y-2 rounded-xl border border-warning/20 bg-warning/10 p-3 text-sm text-warning"
			>
				<div class="flex items-start gap-2">
					<CircleAlert class="mt-0.5 h-4 w-4 shrink-0" />
					<div>
						<p class="font-medium">
							A newer server revision exists (rev {currentServerRev ?? 'unknown'}).
						</p>
						<p class="mt-0.5">
							Your unsaved changes are still in the editor below. Copy or download them before
							reloading the server version.
						</p>
					</div>
				</div>
				<div class="flex flex-wrap gap-2 pl-6">
					<button class="btn rounded-lg btn-warning btn-xs" onclick={copyMyChanges}>
						{#if conflictCopied}
							<Check class="h-3 w-3" />
							<span>Copied!</span>
						{:else}
							<Copy class="h-3 w-3" />
							<span>Copy my changes</span>
						{/if}
					</button>
					<button class="btn rounded-lg btn-warning btn-xs" onclick={downloadMyVersion}>
						<Download class="h-3 w-3" />
						<span>Download my version</span>
					</button>
					<button
						class="btn rounded-lg btn-outline btn-warning btn-xs"
						onclick={confirmReloadFromServer}
					>
						<RotateCcw class="h-3 w-3" />
						<span>Reload server version</span>
					</button>
				</div>
			</div>
		{/if}

		{#if tombstoneConflict}
			<div class="space-y-2 rounded-xl border border-error/20 bg-error/10 p-3 text-sm text-error">
				<div class="flex items-start gap-2">
					<CircleAlert class="mt-0.5 h-4 w-4 shrink-0" />
					<div>
						<p class="font-medium">This file was deleted on the server.</p>
						<p class="mt-0.5">
							It can no longer be saved. Your changes are still in the editor below. Copy or
							download them before closing this file.
						</p>
					</div>
				</div>
				<div class="flex flex-wrap gap-2 pl-6">
					<button class="btn rounded-lg btn-error btn-xs" onclick={copyMyChanges}>
						{#if conflictCopied}
							<Check class="h-3 w-3" />
							<span>Copied!</span>
						{:else}
							<Copy class="h-3 w-3" />
							<span>Copy my changes</span>
						{/if}
					</button>
					<button class="btn rounded-lg btn-error btn-xs" onclick={downloadMyVersion}>
						<Download class="h-3 w-3" />
						<span>Download my version</span>
					</button>
					<button class="btn rounded-lg btn-outline btn-error btn-xs" onclick={closeFile}>
						<X class="h-3 w-3" />
						<span>Close file</span>
					</button>
				</div>
			</div>
		{/if}

		{#if canEdit}
			<div class="h-[min(70vh,50rem)] min-h-[32rem]">
				{#if isMarkdown}
					<RichMarkdownEditor
						bind:this={richEditor}
						content={editorDocument.body}
						currentMarkdown={editorDocument.body}
						editable={!$contentQuery.isLoading && !$saveMutation.isPending}
						hasAttachmentHandler={false}
						on:change={(event) =>
							(localContent = editorDocument.hasFrontmatter
								? wrapFrontmatter(editorDocument.frontmatter, event.detail.markdown)
								: event.detail.markdown)}
					/>
				{:else}
					<textarea
						class="textarea-bordered textarea h-full w-full resize-none rounded-2xl font-mono text-sm"
						bind:value={localContent}
						disabled={$contentQuery.isLoading || $saveMutation.isPending}></textarea>
				{/if}
			</div>
		{:else}
			<pre
				class="min-h-[24rem] overflow-auto rounded-2xl border border-base-300/70 bg-base-200/50 p-4 font-mono text-sm whitespace-pre-wrap">{$contentQuery
					.data?.content ?? ''}</pre>
		{/if}
	</div>
{/if}
