<script lang="ts">
	import { onMount } from 'svelte';
	import {
		serverActivityStore,
		getActivityDisplay,
		getRelativeTime,
		getActivityHref
	} from '$lib/stores/activity';

	let {
		maxItems = 10,
		showHeader = true
	}: {
		maxItems?: number;
		showHeader?: boolean;
	} = $props();

	onMount(() => {
		serverActivityStore.fetch(maxItems);
	});

	function handleLoadMore() {
		serverActivityStore.loadMore(maxItems);
	}
</script>

<div class="space-y-4">
	{#if showHeader}
		<div class="flex items-center justify-between">
			<h3 class="text-lg font-semibold">Recent Activity</h3>
		</div>
	{/if}

	{#if $serverActivityStore.loading && $serverActivityStore.items.length === 0}
		<div class="py-8 text-center">
			<div
				class="inline-block h-6 w-6 animate-spin rounded-full border-2 border-brand-500 border-t-transparent"
			></div>
		</div>
	{:else if $serverActivityStore.error && $serverActivityStore.items.length === 0}
		<div class="py-8 text-center text-error">
			<p class="text-sm">{$serverActivityStore.error}</p>
		</div>
	{:else if $serverActivityStore.items.length === 0}
		<div class="py-8 text-center text-base-content/60">
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="mx-auto mb-2 h-12 w-12 opacity-50"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
				/>
			</svg>
			<p class="text-[12px]">No recent activity</p>
		</div>
	{:else}
		<div class="space-y-2">
			{#each $serverActivityStore.items as activity (activity.id)}
				{@const display = getActivityDisplay(activity)}
				{@const href = getActivityHref(activity)}
				{#if href}
					<a
						{href}
						class="group flex items-start gap-3 rounded-lg p-3 transition-colors hover:bg-base-200/50"
					>
						<div class="mt-0.5 flex-shrink-0 text-lg">
							{#if typeof display.icon === 'string'}
								{display.icon}
							{:else}
								<svelte:component
									this={display.icon}
									size={18}
									style={display.color.startsWith('#') ? `color: ${display.color}` : undefined}
									class={!display.color.startsWith('#') ? display.color : undefined}
								/>
							{/if}
						</div>
						<div class="min-w-0 flex-1">
							{#if display.color.startsWith('#')}
								<p class="text-[13px] leading-tight font-medium" style="color: {display.color}">
									{display.title}
								</p>
							{:else}
								<p class="text-[13px] leading-tight font-medium {display.color}">
									{display.title}
								</p>
							{/if}
							<p class="mt-0.5 truncate text-[12px] text-base-content/70">
								{display.description}
							</p>
							<p
								class="mt-1 text-[10px] font-semibold tracking-wider text-base-content/40 uppercase"
							>
								{getRelativeTime(activity.timestamp)}
							</p>
						</div>
					</a>
				{:else}
					<div
						class="group flex cursor-not-allowed items-start gap-3 rounded-lg p-3 opacity-60"
						title="This activity record cannot be opened"
					>
						<div class="mt-0.5 flex-shrink-0 text-lg">
							{#if typeof display.icon === 'string'}
								{display.icon}
							{:else}
								<svelte:component
									this={display.icon}
									size={18}
									style={display.color.startsWith('#') ? `color: ${display.color}` : undefined}
									class={!display.color.startsWith('#') ? display.color : undefined}
								/>
							{/if}
						</div>
						<div class="min-w-0 flex-1">
							{#if display.color.startsWith('#')}
								<p class="text-[13px] leading-tight font-medium" style="color: {display.color}">
									{display.title}
								</p>
							{:else}
								<p class="text-[13px] leading-tight font-medium {display.color}">
									{display.title}
								</p>
							{/if}
							<p class="mt-0.5 truncate text-[12px] text-base-content/70">
								{display.description}
							</p>
							<p
								class="mt-1 text-[10px] font-semibold tracking-wider text-base-content/40 uppercase"
							>
								{getRelativeTime(activity.timestamp)}
							</p>
						</div>
					</div>
				{/if}
			{/each}
		</div>

		{#if $serverActivityStore.hasMore}
			<div class="pt-2 text-center">
				<button
					class="btn btn-ghost btn-sm text-xs"
					onclick={handleLoadMore}
					disabled={$serverActivityStore.loading}
				>
					{#if $serverActivityStore.loading}
						<span
							class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-brand-500 border-t-transparent"
						></span>
					{:else}
						Load more
					{/if}
				</button>
			</div>
		{/if}
	{/if}
</div>
