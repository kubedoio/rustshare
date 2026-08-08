<script lang="ts">
	import { onMount } from 'svelte';
	import {
		serverActivityStore,
		getActivityDisplay,
		getRelativeTime,
		getActivityHref
	} from '$lib/stores/activity';
	import { getActivityVerb, getApplicationColor } from '$lib/utils/dashboard';
	import DashboardSectionHeader from './DashboardSectionHeader.svelte';
	import DashboardEmptyState from './DashboardEmptyState.svelte';

	let {
		userName: _userName = undefined,
		nameLookup = undefined
	}: {
		userName?: string | undefined;
		nameLookup?: Map<string, string> | undefined;
	} = $props();

	onMount(() => {
		serverActivityStore.fetch(6);
	});

	function getActivityName(activity: {
		fileName: string;
		artifactId?: string;
		resourceType?: string;
		type?: string;
	}): string {
		if (activity.fileName && activity.fileName !== 'Unknown') {
			return activity.fileName;
		}

		if (activity.artifactId) {
			const resolved = nameLookup?.get(activity.artifactId);
			if (resolved) {
				return resolved;
			}
		}

		// Neutral fallback instead of the raw 'Unknown' placeholder (share events
		// arrive without a resource name). Folders get their own label when known.
		return activity.resourceType === 'folder' || activity.type?.startsWith('folder_')
			? 'A folder'
			: 'A file';
	}
</script>

<section class="recent-activity" aria-label="Recent activity">
	<DashboardSectionHeader
		title="Recent activity"
		onClick={() => {
			window.location.href = '/settings?tab=activity';
		}}
	/>
	{#if $serverActivityStore.loading}
		<div class="py-6 text-center">
			<div
				class="inline-block h-5 w-5 animate-spin rounded-full border-2 border-brand-500 border-t-transparent"
			></div>
		</div>
	{:else if $serverActivityStore.error}
		<DashboardEmptyState description={$serverActivityStore.error} minimal />
	{:else if $serverActivityStore.items.length === 0}
		<DashboardEmptyState
			description="Activity will appear here as you work in your workspace."
			minimal
		/>
	{:else}
		<ul class="activity-list">
			{#each $serverActivityStore.items as activity}
				{@const href = getActivityHref(activity)}
				{@const display = getActivityDisplay(activity)}
				{@const moduleColor = getApplicationColor(activity.applicationId ?? '')}
				{@const activityName = getActivityName(activity)}
				{#if href}
					<li>
						<a
							{href}
							class="activity-item activity-item--clickable"
							aria-label="Open {activityName}"
						>
							<div
								class="activity-icon-wrap"
								style="background: {moduleColor.bg}; color: {moduleColor.color};"
							>
								{#if typeof display.icon === 'string'}
									{display.icon}
								{:else}
									<svelte:component this={display.icon} size={16} />
								{/if}
							</div>
							<div class="activity-body">
								<span class="activity-name">{activityName}</span>
								<span class="activity-description">
									<span class="activity-actor">You</span>
									<span>{getActivityVerb(activity.type)}</span>
								</span>
							</div>
							<span class="activity-time">{getRelativeTime(activity.timestamp)}</span>
						</a>
					</li>
				{:else}
					<li>
						<div
							class="activity-item activity-item--stale"
							title="This activity record cannot be opened"
						>
							<div
								class="activity-icon-wrap"
								style="background: {moduleColor.bg}; color: {moduleColor.color};"
							>
								{#if typeof display.icon === 'string'}
									{display.icon}
								{:else}
									<svelte:component this={display.icon} size={16} />
								{/if}
							</div>
							<div class="activity-body">
								<span class="activity-name">{activityName}</span>
								<span class="activity-description">
									<span class="activity-actor">You</span>
									<span>{getActivityVerb(activity.type)}</span>
								</span>
							</div>
							<span class="activity-time">{getRelativeTime(activity.timestamp)}</span>
						</div>
					</li>
				{/if}
			{/each}
		</ul>
	{/if}
</section>

<style>
	.recent-activity {
		border: 1px solid color-mix(in oklab, var(--base-300) 52%, transparent);
		border-radius: 0.5rem;
		background: color-mix(in oklab, var(--base-100) 94%, white);
		overflow: hidden;
	}
	.recent-activity :global(.dashboard-section-header) {
		padding: 1rem 1rem 0.75rem;
	}
	.activity-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
	}
	.activity-item {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		min-height: 4.25rem;
		padding: 0.75rem 1rem;
		border-top: 1px solid color-mix(in oklab, var(--base-300) 44%, transparent);
	}
	.activity-item--clickable {
		cursor: pointer;
		text-decoration: none;
		color: inherit;
		transition: background 150ms ease;
	}
	.activity-item--clickable:hover {
		background: color-mix(in oklab, var(--brand-500) 4%, var(--base-100));
	}
	.activity-item--clickable:focus-visible {
		outline: 2px solid color-mix(in oklab, var(--brand-500) 72%, transparent);
		outline-offset: -2px;
	}
	.activity-item--stale {
		opacity: 0.7;
		cursor: not-allowed;
	}
	.activity-icon-wrap {
		font-size: 1rem;
		line-height: 1;
		flex-shrink: 0;
		width: 2rem;
		height: 2rem;
		border-radius: 0.45rem;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.activity-body {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		min-width: 0;
		flex: 1;
	}
	.activity-name {
		font-size: 0.86rem;
		font-weight: 650;
		color: var(--base-content);
		line-height: 1.35;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.activity-description {
		display: flex;
		align-items: baseline;
		gap: 0.25rem;
		font-size: 0.74rem;
		line-height: 1.35;
		color: color-mix(in oklab, var(--base-content) 56%, transparent);
	}
	.activity-actor {
		font-weight: 600;
		color: color-mix(in oklab, var(--base-content) 70%, transparent);
	}
	.activity-time {
		font-size: 0.72rem;
		line-height: 1.35;
		color: color-mix(in oklab, var(--base-content) 45%, transparent);
		flex-shrink: 0;
		white-space: nowrap;
	}

	@media (max-width: 520px) {
		.activity-item {
			align-items: flex-start;
		}

		.activity-time {
			padding-top: 0.1rem;
		}
	}
</style>
