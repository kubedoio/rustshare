<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { listVaults } from '$lib/api/vaults';
	import type { Vault } from '$lib/api/types';
	import { Archive, ChevronRight, Database } from 'lucide-svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';

	interface VaultListResponse {
		vaults: Vault[];
	}

	const vaultsQuery = createQuery<VaultListResponse>({
		queryKey: ['vaults'],
		queryFn: () => listVaults()
	});

	function adapterLabel(adapter: string): string {
		if (adapter === 'obsidian_vault') return 'Obsidian vault';
		return adapter;
	}

	function adapterBadgeClass(adapter: string): string {
		if (adapter === 'obsidian_vault')
			return 'border-purple-500/20 bg-purple-500/10 text-purple-600';
		return 'border-base-300 bg-base-200 text-base-content/70';
	}
</script>

<svelte:head>
	<title>Vaults - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-5xl space-y-6">
	<!-- Header -->
	<div
		class="overflow-hidden rounded-[2rem] border border-base-300/70 bg-gradient-to-br from-base-100 via-base-100 to-base-200/80 shadow-panel"
	>
		<div class="flex flex-col gap-6 p-6 lg:flex-row lg:items-end lg:justify-between lg:p-8">
			<div class="max-w-2xl">
				<div class="rs-kicker mb-4">
					<Database class="h-3.5 w-3.5" />
					Vault Sync
				</div>
				<h1
					class="font-display text-4xl leading-[0.97] tracking-tight text-base-content lg:text-5xl"
				>
					Your vaults
				</h1>
				<p class="mt-4 max-w-xl text-sm leading-6 text-base-content/68 lg:text-base">
					Sync and manage your external knowledge bases. Each vault is a self-contained workspace
					that stays in sync across your devices.
				</p>
			</div>
		</div>
	</div>

	<!-- Vaults List -->
	{#if $vaultsQuery.isLoading}
		<div class="space-y-4">
			{#each Array.from({ length: 2 }) as _, i (i)}
				<div class="rounded-[1.75rem] border border-base-300/70 bg-base-100 p-5 shadow-sm">
					<div class="flex items-center gap-4">
						<div class="h-10 w-10 animate-pulse rounded-2xl bg-base-300/60"></div>
						<div class="flex-1 space-y-2">
							<div class="h-5 w-48 animate-pulse rounded bg-base-300/60"></div>
							<div class="h-4 w-32 animate-pulse rounded bg-base-300/60"></div>
						</div>
					</div>
				</div>
			{/each}
		</div>
	{:else if $vaultsQuery.isError}
		<ErrorState
			title="Failed to load vaults"
			message={$vaultsQuery.error?.message || 'Unknown error'}
			onRetry={() => $vaultsQuery.refetch()}
		/>
	{:else if $vaultsQuery.data && $vaultsQuery.data.vaults.length === 0}
		<div
			class="rounded-[2rem] border border-dashed border-base-300 bg-base-100 px-6 py-16 text-center shadow-sm"
		>
			<div
				class="mx-auto mb-5 flex h-16 w-16 items-center justify-center rounded-3xl bg-brand-500/10 text-brand-500"
			>
				<Archive class="h-8 w-8" />
			</div>
			<h3 class="font-display text-3xl text-base-content">No vaults yet</h3>
			<p class="mx-auto mt-3 max-w-md font-data text-sm leading-6 text-base-content/65">
				Vaults let you sync external knowledge bases with RustShare. Create your first vault to get
				started.
			</p>
		</div>
	{:else if $vaultsQuery.data}
		<div class="space-y-4">
			{#each $vaultsQuery.data.vaults as vault}
				<a
					href="/vaults/{vault.id}"
					class="flex items-center gap-4 rounded-[1.75rem] border border-base-300/70 bg-base-100 p-5 shadow-sm transition-colors hover:border-brand-500/15"
				>
					<div
						class="flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl border border-base-300/70 bg-base-200/70 text-brand-500"
					>
						<Archive class="h-6 w-6" />
					</div>

					<div class="min-w-0 flex-1">
						<div class="flex flex-wrap items-center gap-2">
							<h2 class="truncate font-display text-xl leading-none text-base-content">
								{vault.name}
							</h2>
							<span
								class="rounded-full border px-2.5 py-0.5 text-xs font-medium {adapterBadgeClass(
									vault.adapter
								)}"
							>
								{adapterLabel(vault.adapter)}
							</span>
						</div>
						<div class="mt-2 flex items-center gap-3 text-sm text-base-content/60">
							<span class="font-data">rev {vault.server_rev}</span>
							{#if vault.root_path}
								<span class="text-base-content/30">|</span>
								<span class="font-data">{vault.root_path}</span>
							{/if}
						</div>
					</div>

					<ChevronRight class="h-5 w-5 shrink-0 text-base-content/30" />
				</a>
			{/each}
		</div>
	{/if}
</div>
