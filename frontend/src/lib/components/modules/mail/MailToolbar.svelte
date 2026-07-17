<script lang="ts">
	import {
		ChevronDown,
		Mail,
		MoreHorizontal,
		RefreshCw,
		Search,
		PenSquare,
		Upload,
		Settings,
		UserPlus,
		Archive,
		Loader2,
		X
	} from 'lucide-svelte';
	import type { MailAccount } from '$lib/api/mail';

	let {
		accounts,
		selectedAccountId,
		searchValue,
		refreshing = false,
		activeJobLabel = null,
		onSelectAccount,
		onSearch,
		onClearSearch,
		onRefresh,
		onCompose,
		onUploadEml,
		onOpenActivity
	}: {
		accounts: MailAccount[];
		selectedAccountId: string | null;
		searchValue: string;
		refreshing?: boolean;
		activeJobLabel?: string | null;
		onSelectAccount: (accountId: string) => void;
		onSearch: (value: string) => void;
		onClearSearch: () => void;
		onRefresh: () => void;
		onCompose: () => void;
		onUploadEml: () => void;
		onOpenActivity: () => void;
	} = $props();

	let accountMenuOpen = $state(false);
	let overflowOpen = $state(false);
	let searchInput = $state('');

	$effect(() => {
		searchInput = searchValue;
	});

	let selectedAccount = $derived(accounts.find((account) => account.id === selectedAccountId));

	function closeMenus() {
		accountMenuOpen = false;
		overflowOpen = false;
	}

	function handleWindowClick(event: MouseEvent) {
		if (!(event.target as HTMLElement | null)?.closest('[data-mail-menu]')) closeMenus();
	}
</script>

<svelte:window onclick={handleWindowClick} />

<div
	class="flex flex-wrap items-center gap-2 border-b border-[var(--rs-border)] bg-[var(--rs-surface-raised)] px-3 py-2"
>
	<div class="flex min-w-0 items-center gap-2">
		<span
			class="flex h-7 w-7 items-center justify-center rounded-md bg-brand-500/10 text-brand-600"
		>
			<Mail size={15} />
		</span>
		<h1 class="text-sm font-semibold text-base-content">Mail</h1>
	</div>

	<!-- Account selector -->
	<div class="relative" data-mail-menu>
		<button
			type="button"
			class="btn btn-sm btn-outline max-w-52 gap-1.5 font-normal"
			aria-haspopup="listbox"
			aria-expanded={accountMenuOpen}
			aria-label="Select mail account"
			onclick={() => {
				overflowOpen = false;
				accountMenuOpen = !accountMenuOpen;
			}}
		>
			<span class="truncate">
				{selectedAccount ? selectedAccount.name : 'Select account'}
			</span>
			<ChevronDown size={13} class="shrink-0 opacity-60" />
		</button>
		{#if accountMenuOpen}
			<ul
				class="absolute left-0 z-30 mt-1 max-h-72 w-72 overflow-auto rounded-lg border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] py-1 shadow-lg"
				role="listbox"
				aria-label="Mail accounts"
			>
				{#each accounts as account}
					<li>
						<button
							type="button"
							role="option"
							aria-selected={account.id === selectedAccountId}
							class="flex w-full min-w-0 flex-col px-3 py-2 text-left hover:bg-base-200 {account.id ===
							selectedAccountId
								? 'bg-base-200'
								: ''}"
							onclick={() => {
								onSelectAccount(account.id);
								closeMenus();
							}}
						>
							<span class="truncate text-sm font-medium text-base-content">{account.name}</span>
							<span class="truncate text-xs text-base-content/55">{account.username}</span>
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	<!-- Search -->
	<form
		class="relative min-w-40 flex-1"
		onsubmit={(event) => {
			event.preventDefault();
			onSearch(searchInput.trim());
		}}
	>
		<Search
			size={14}
			class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-base-content/40"
		/>
		<input
			type="search"
			class="input input-sm input-bordered h-8 w-full pl-8"
			placeholder="Search messages"
			aria-label="Search messages"
			bind:value={searchInput}
		/>
		{#if searchValue}
			<button
				type="button"
				class="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-base-content/50 hover:bg-base-200"
				aria-label="Clear search"
				onclick={() => {
					searchInput = '';
					onClearSearch();
				}}
			>
				<X size={13} />
			</button>
		{/if}
	</form>

	{#if activeJobLabel}
		<button
			type="button"
			class="btn btn-sm btn-ghost gap-1.5 text-xs text-brand-600"
			onclick={onOpenActivity}
		>
			<Loader2 size={13} class="animate-spin" />
			<span class="hidden sm:inline">{activeJobLabel}</span>
		</button>
	{/if}

	<div class="ml-auto flex items-center gap-1.5">
		<button
			type="button"
			class="btn btn-sm btn-ghost btn-square"
			title="Refresh current view"
			aria-label="Refresh current view"
			disabled={refreshing}
			onclick={onRefresh}
		>
			<RefreshCw size={15} class={refreshing ? 'animate-spin' : ''} />
		</button>
		<button type="button" class="btn btn-sm btn-primary gap-1.5" onclick={onCompose}>
			<PenSquare size={14} />
			<span class="hidden sm:inline">Compose</span>
		</button>

		<!-- Overflow menu -->
		<div class="relative" data-mail-menu>
			<button
				type="button"
				class="btn btn-sm btn-ghost btn-square"
				aria-haspopup="menu"
				aria-expanded={overflowOpen}
				aria-label="More mail actions"
				onclick={() => {
					accountMenuOpen = false;
					overflowOpen = !overflowOpen;
				}}
			>
				<MoreHorizontal size={16} />
			</button>
			{#if overflowOpen}
				<ul
					class="absolute right-0 z-30 mt-1 w-56 rounded-lg border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] py-1 shadow-lg"
					role="menu"
				>
					<li>
						<button
							type="button"
							role="menuitem"
							class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-base-200"
							onclick={() => {
								onUploadEml();
								closeMenus();
							}}
						>
							<Upload size={14} class="text-base-content/50" /> Upload .eml
						</button>
					</li>
					<li>
						<button
							type="button"
							role="menuitem"
							class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-base-200"
							onclick={() => {
								onOpenActivity();
								closeMenus();
							}}
						>
							<Archive size={14} class="text-base-content/50" /> Archive activity
						</button>
					</li>
					<li class="my-1 border-t border-[var(--rs-border)]"></li>
					<li>
						<a
							href="/settings?tab=mail"
							role="menuitem"
							class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-base-200"
							onclick={closeMenus}
						>
							<UserPlus size={14} class="text-base-content/50" /> Add mail account
						</a>
					</li>
					<li>
						<a
							href="/settings?tab=mail"
							role="menuitem"
							class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-base-200"
							onclick={closeMenus}
						>
							<Settings size={14} class="text-base-content/50" /> Mail account settings
						</a>
					</li>
				</ul>
			{/if}
		</div>
	</div>
</div>
