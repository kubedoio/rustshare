<script lang="ts">
	import { goto } from '$app/navigation';
	import { getActivityDisplay, getRelativeTime, type Activity } from '$lib/stores/activity';
	import { getActivityVerb, getUserInitials } from '$lib/utils/dashboard';
	import DashboardSectionHeader from './DashboardSectionHeader.svelte';
	import DashboardEmptyState from './DashboardEmptyState.svelte';

	export let activities: Activity[];
	export let userName: string | undefined = undefined;

	function getActivityHref(activity: Activity): string | null {
		if (!activity.artifactId) return null;

		switch (activity.moduleKey) {
			case 'notes':
				return `/modules/notes/${activity.artifactId}`;
			case 'meetings':
				return `/modules/meetings/${activity.artifactId}`;
			case 'standups':
				return `/modules/standups/${activity.artifactId}`;
			case 'decisions':
				return `/modules/decisions/${activity.artifactId}`;
			case 'brainstorming':
				return `/modules/brainstorming/${activity.artifactId}`;
			case 'kanban':
				return `/modules/kanban?boardId=${activity.artifactId}`;
			case 'shares':
				return `/modules/shares/${activity.artifactId}`;
			default:
				// Fallback for file-system artifacts
				return `/files?preview=${activity.artifactId}`;
		}
	}
</script>

<section class="recent-activity" aria-label="Recent activity">
	<DashboardSectionHeader
		title="Recent activity"
		onClick={() => {
			window.location.href = '/settings?tab=activity';
		}}
	/>
	{#if activities.length === 0}
		<DashboardEmptyState
			description="Activity will appear here as you work in your workspace."
			minimal
		/>
	{:else}
		<ul class="activity-list">
			{#each activities as activity}
				{@const href = getActivityHref(activity)}
				{@const display = getActivityDisplay(activity)}
				{#if href}
					<a
						{href}
						class="activity-item activity-item--clickable"
						aria-label="Open {activity.fileName}"
					>
						<div class="activity-icon-wrap">
							{#if typeof display.icon === 'string'}
								{display.icon}
							{:else}
								<svelte:component this={display.icon} size={16} />
							{/if}
						</div>
						<div class="activity-body">
							<span class="activity-text">
								<strong>{activity.fileName}</strong> {getActivityVerb(activity.type)}
							</span>
							<span class="activity-time">{getRelativeTime(activity.timestamp)}</span>
						</div>
						<span class="activity-user-avatar">
							{getUserInitials(userName)}
						</span>
					</a>
				{:else}
					<li class="activity-item activity-item--stale" title="This activity record cannot be opened">
						<div class="activity-icon-wrap">
							{display.icon}
						</div>
						<div class="activity-body">
							<span class="activity-text">
								<strong>{activity.fileName}</strong> {getActivityVerb(activity.type)}
							</span>
							<span class="activity-time">{getRelativeTime(activity.timestamp)}</span>
						</div>
						<span class="activity-user-avatar">
							{getUserInitials(userName)}
						</span>
					</li>
				{/if}
			{/each}
		</ul>
	{/if}
</section>

<style>
	.activity-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.activity-item {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.6rem 0.75rem;
		border-radius: 0.65rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 50%, white);
	}
	.activity-item--clickable {
		cursor: pointer;
		text-decoration: none;
		color: inherit;
	}
	.activity-item--clickable:hover {
		border-color: color-mix(in oklab, var(--brand-500) 30%, transparent);
		background: color-mix(in oklab, var(--brand-500) 4%, white);
	}
	.activity-item--stale {
		opacity: 0.7;
		cursor: not-allowed;
	}
	.activity-icon-wrap {
		font-size: 1rem;
		line-height: 1;
		flex-shrink: 0;
		width: 1.75rem;
		text-align: center;
	}
	.activity-body {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		min-width: 0;
		flex: 1;
	}
	.activity-text {
		font-size: 0.8rem;
		color: var(--base-content);
		line-height: 1.4;
		overflow: hidden;
		display: -webkit-box;
		line-clamp: 2;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
	}
	.activity-text strong {
		font-weight: 600;
	}
	.activity-time {
		font-size: 0.7rem;
		color: color-mix(in oklab, var(--base-content) 45%, transparent);
	}
	.activity-user-avatar {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 999px;
		background: color-mix(in oklab, var(--brand-500) 15%, transparent);
		color: var(--brand-500);
		font-size: 0.65rem;
		font-weight: 700;
		flex-shrink: 0;
	}
</style>
