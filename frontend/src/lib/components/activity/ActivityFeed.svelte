<script lang="ts">
	import { activityStore, getActivityDisplay, getRelativeTime } from '$lib/stores/activity';
	import type { Activity } from '$lib/stores/activity';
	import { isInternalRustShareFile } from '$lib/utils/artifactVisibility';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';

	export let maxItems = 10;
	export let showClearButton = true;
	export let showHeader = true;

	let showConfirmModal = false;

	$: recentActivities = $activityStore.filter((a) => !isInternalRustShareFile(a.fileName)).slice(0, maxItems);

	function handleClearHistory() {
		showConfirmModal = true;
	}

	function onConfirmClear() {
		activityStore.clearHistory();
		showConfirmModal = false;
	}

	function handleRemoveActivity(id: string) {
		activityStore.removeActivity(id);
	}
</script>

<div class="space-y-4">
	<!-- Header -->
	{#if showHeader}
		<div class="flex items-center justify-between">
			<h3 class="text-lg font-semibold">Recent Activity</h3>
			{#if showClearButton && $activityStore.length > 0}
				<button
					class="btn text-[11px] btn-ghost btn-xs"
					on:click={handleClearHistory}
					title="Clear all history"
				>
					Clear All
				</button>
			{/if}
		</div>
	{/if}

	<!-- Activity List -->
	{#if recentActivities.length === 0}
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
			{#each recentActivities as activity (activity.id)}
				{@const display = getActivityDisplay(activity)}
				<div
					class="group flex items-start gap-3 rounded-lg p-3 transition-colors hover:bg-base-200/50"
				>
					<div class="mt-0.5 flex-shrink-0 text-lg">
						{display.icon}
					</div>
					<div class="min-w-0 flex-1">
						<p class="text-[13px] leading-tight font-medium {display.color}">
							{display.title}
						</p>
						<p class="mt-0.5 truncate text-[12px] text-base-content/70">
							{display.description}
						</p>
						<p class="mt-1 text-[10px] font-semibold tracking-wider text-base-content/40 uppercase">
							{getRelativeTime(activity.timestamp)}
						</p>
					</div>
					<button
						class="btn btn-circle opacity-0 btn-ghost transition-opacity btn-xs group-hover:opacity-100"
						on:click={() => handleRemoveActivity(activity.id)}
						title="Remove"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							fill="none"
							viewBox="0 0 24 24"
							stroke-width="1.5"
							stroke="currentColor"
							class="h-4 w-4 text-base-content/50 hover:text-error"
						>
							<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
						</svg>
					</button>
				</div>
			{/each}
		</div>

		{#if $activityStore.length > maxItems}
			<div class="pt-2 text-center">
				<p class="text-[10px] font-medium tracking-wider text-base-content/40 uppercase">
					Showing {maxItems} of {$activityStore.length} activities
				</p>
			</div>
		{/if}
	{/if}
</div>

<ConfirmModal
	open={showConfirmModal}
	title="Clear Activity History"
	message="Clear all activity history? This cannot be undone."
	confirmLabel="Clear All"
	cancelLabel="Cancel"
	danger={true}
	onConfirm={onConfirmClear}
	onCancel={() => showConfirmModal = false}
/>
