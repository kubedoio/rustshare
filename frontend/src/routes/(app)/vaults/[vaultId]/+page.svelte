<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery, createMutation } from '$lib/query-compat';
	import { getVault, getManifest, updateVaultWritePolicy } from '$lib/api/vaults';
	import { queryClient } from '$lib/query-client';
	import type { Vault, VaultManifest, VaultManifestEntry, VaultWritePolicy } from '$lib/api/types';
	import { formatFileSize } from '$lib/utils/format';
	import { isEditableVaultFile, isEditableVaultPolicy } from '$lib/utils/vault';
	import { Archive, ArrowLeft, FileText, Database, Trash2, Pencil, Eye } from 'lucide-svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import VaultFileEditor from '$lib/components/vaults/VaultFileEditor.svelte';

	let vaultId = $derived($page.params.vaultId);

	const vaultQuery = createQuery<Vault>({
		queryKey: ['vault', vaultId],
		queryFn: () => getVault(vaultId!),
		enabled: !!vaultId
	});

	const manifestQuery = createQuery<VaultManifest>({
		queryKey: ['vault-manifest', vaultId],
		queryFn: () => getManifest(vaultId!),
		enabled: !!vaultId
	});

	$effect(() => {
		vaultQuery.setOptions({
			queryKey: ['vault', vaultId],
			queryFn: () => getVault(vaultId!),
			enabled: !!vaultId
		});
	});

	$effect(() => {
		manifestQuery.setOptions({
			queryKey: ['vault-manifest', vaultId],
			queryFn: () => getManifest(vaultId!),
			enabled: !!vaultId
		});
	});

	function adapterLabel(adapter: string): string {
		if (adapter === 'ObsidianVault') return 'Obsidian vault';
		return adapter;
	}

	function adapterBadgeClass(adapter: string): string {
		if (adapter === 'ObsidianVault') return 'border-purple-500/20 bg-purple-500/10 text-purple-600';
		return 'border-base-300 bg-base-200 text-base-content/70';
	}

	let vault = $derived($vaultQuery.data);
	let manifest = $derived($manifestQuery.data);
	let isLoading = $derived($vaultQuery.isLoading || $manifestQuery.isLoading);
	let isError = $derived($vaultQuery.isError || $manifestQuery.isError);
	let errorMessage = $derived(
		$vaultQuery.error?.message || $manifestQuery.error?.message || 'Unknown error'
	);

	let selectedFile = $state<VaultManifestEntry | null>(null);

	function selectFile(file: VaultManifestEntry) {
		selectedFile = file;
	}

	const writePolicyOptions: { value: VaultWritePolicy; label: string }[] = [
		{ value: 'read_only', label: 'Read-only' },
		{ value: 'web_editing_enabled', label: 'Web editing enabled' },
		{ value: 'sync_client_only', label: 'Sync client only' }
	];

	const updatePolicyMutation = createMutation({
		mutationFn: (policy: VaultWritePolicy) => updateVaultWritePolicy(vaultId!, policy),
		onSuccess: (updatedVault) => {
			queryClient.setQueryData(['vault', vaultId], updatedVault);
		}
	});

	function onPolicyChange(event: Event) {
		const select = event.target as HTMLSelectElement;
		$updatePolicyMutation.mutate(select.value as VaultWritePolicy);
	}
</script>

<svelte:head>
	<title>{vault?.name ?? 'Vault'} - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-5xl space-y-6">
	<!-- Back link -->
	<a
		href="/vaults"
		class="inline-flex items-center gap-2 text-sm font-medium text-base-content/60 transition-colors hover:text-base-content"
	>
		<ArrowLeft class="h-4 w-4" />
		Back to vaults
	</a>

	{#if isLoading}
		<div class="space-y-6">
			<div class="rounded-[2rem] border border-base-300/70 bg-base-100 p-6 shadow-sm lg:p-8">
				<div class="flex items-center gap-4">
					<div class="h-12 w-12 animate-pulse rounded-2xl bg-base-300/60"></div>
					<div class="flex-1 space-y-2">
						<div class="h-7 w-56 animate-pulse rounded bg-base-300/60"></div>
						<div class="h-4 w-32 animate-pulse rounded bg-base-300/60"></div>
					</div>
				</div>
			</div>
			<div class="space-y-3">
				{#each Array.from({ length: 3 }) as _, i (i)}
					<div class="rounded-[1.25rem] border border-base-300/70 bg-base-100 p-4 shadow-sm">
						<div class="flex items-center gap-3">
							<div class="h-5 w-5 animate-pulse rounded bg-base-300/60"></div>
							<div class="h-4 w-64 animate-pulse rounded bg-base-300/60"></div>
						</div>
					</div>
				{/each}
			</div>
		</div>
	{:else if isError}
		<ErrorState
			title="Failed to load vault"
			message={errorMessage}
			onRetry={() => {
				$vaultQuery.refetch();
				$manifestQuery.refetch();
			}}
		/>
	{:else if vault}
		<!-- Vault Header -->
		<div
			class="overflow-hidden rounded-[2rem] border border-base-300/70 bg-gradient-to-br from-base-100 via-base-100 to-base-200/80 shadow-panel"
		>
			<div class="flex flex-col gap-6 p-6 lg:flex-row lg:items-start lg:justify-between lg:p-8">
				<div class="flex items-start gap-4">
					<div
						class="flex h-14 w-14 shrink-0 items-center justify-center rounded-2xl border border-base-300/70 bg-base-200/70 text-brand-500"
					>
						<Archive class="h-7 w-7" />
					</div>
					<div class="min-w-0">
						<div class="flex flex-wrap items-center gap-2">
							<h1
								class="font-display text-3xl leading-[0.97] tracking-tight text-base-content lg:text-4xl"
							>
								{vault.name}
							</h1>
							<span
								class="rounded-full border px-2.5 py-0.5 text-xs font-medium {adapterBadgeClass(
									vault.adapter
								)}"
							>
								{adapterLabel(vault.adapter)}
							</span>
						</div>
						{#if vault.root_path}
							<p class="mt-2 font-data text-sm text-base-content/60">{vault.root_path}</p>
						{/if}
					</div>
				</div>

				<div class="flex flex-wrap gap-3">
					<div class="rounded-2xl border border-base-300/70 bg-base-100 px-4 py-3">
						<p class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
							Server Rev
						</p>
						<p class="mt-1 font-data text-sm font-medium text-base-content">
							{vault.server_rev}
						</p>
					</div>
					<div class="rounded-2xl border border-base-300/70 bg-base-100 px-4 py-3">
						<p class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
							Files
						</p>
						<p class="mt-1 font-data text-sm font-medium text-base-content">
							{manifest?.files.filter((f) => !f.deleted).length ?? 0}
						</p>
					</div>
				</div>
			</div>
		</div>

		<div class="rounded-[1.5rem] border border-base-300/70 bg-base-100 p-4 shadow-sm">
			<label for="vault-write-policy" class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
				Write policy
			</label>
			<select
				id="vault-write-policy"
				class="mt-1 select select-sm select-bordered font-data text-sm"
				value={vault.write_policy}
				onchange={onPolicyChange}
				disabled={$updatePolicyMutation.isPending}
			>
				{#each writePolicyOptions as option}
					<option value={option.value}>{option.label}</option>
				{/each}
			</select>
			{#if $updatePolicyMutation.isError}
				<p class="mt-2 text-xs text-error">Failed to update policy. Please try again.</p>
			{/if}
		</div>

		<!-- Manifest Files -->
		<div class="space-y-3">
			<h2 class="px-1 font-display text-2xl text-base-content">Manifest</h2>

			{#if manifest && manifest.files.length > 0}
				<div class="space-y-2">
					{#each manifest.files as file}
						<button
							class="flex w-full items-center gap-3 rounded-[1.25rem] border border-base-300/70 bg-base-100 p-4 text-left shadow-sm transition-colors hover:bg-base-200/50"
							class:ring-2={selectedFile?.path === file.path}
							class:ring-brand-500={selectedFile?.path === file.path}
							onclick={() => selectFile(file)}
						>
							<div
								class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-base-300/70 bg-base-200/70"
								class:text-error={file.deleted}
								class:text-brand-500={!file.deleted}
							>
								{#if file.deleted}
									<Trash2 class="h-4 w-4" />
								{:else if isEditableVaultFile(file)}
									<Pencil class="h-4 w-4" />
								{:else}
									<Eye class="h-4 w-4" />
								{/if}
							</div>

							<div class="min-w-0 flex-1">
								<div class="flex flex-wrap items-center gap-2">
									<span
										class="truncate font-data text-sm font-medium {file.deleted
											? 'text-base-content/40 line-through'
											: 'text-base-content'}"
									>
										{file.path}
									</span>
									{#if file.deleted}
										<span
											class="rounded-full border border-error/20 bg-error/10 px-2 py-0.5 text-xs font-medium text-error"
										>
											Deleted
										</span>
									{/if}
								</div>
								<div class="mt-1 flex items-center gap-3 text-xs text-base-content/50">
									<span class="font-data">rev {file.server_rev}</span>
									{#if file.size !== undefined && file.size !== null}
										<span class="text-base-content/30">|</span>
										<span class="font-data">{formatFileSize(file.size)}</span>
									{/if}
									{#if file.content_type}
										<span class="text-base-content/30">|</span>
										<span class="font-data">{file.content_type}</span>
									{/if}
								</div>
							</div>
						</button>
					{/each}
				</div>
			{:else}
				<div
					class="rounded-[2rem] border border-dashed border-base-300 bg-base-100 px-6 py-12 text-center shadow-sm"
				>
					<div
						class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl bg-brand-500/10 text-brand-500"
					>
						<Database class="h-6 w-6" />
					</div>
					<h3 class="font-display text-xl text-base-content">Empty manifest</h3>
					<p class="mx-auto mt-2 max-w-md font-data text-sm text-base-content/65">
						This vault has no files in its manifest yet. Files will appear here once they are
						synced.
					</p>
				</div>
			{/if}
		</div>

		<div class="rounded-[2rem] border border-base-300/70 bg-base-100 p-6 shadow-sm lg:p-8">
			<VaultFileEditor vaultId={vaultId!} policy={vault.write_policy} file={selectedFile} />
		</div>
	{:else}
		<ErrorState title="Vault not found" message="The requested vault does not exist." />
	{/if}
</div>
