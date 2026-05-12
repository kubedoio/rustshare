<script lang="ts">
	import { formatDistanceToNow } from 'date-fns';
	import { ArrowUpRight } from 'lucide-svelte';
	import {
		getModuleColor,
		getArtifactIcon,
		getArtifactTypeLabel,
		getArtifactHref,
		cleanArtifactName,
		getUserInitials
	} from '$lib/utils/dashboard';
	import DashboardSectionHeader from './DashboardSectionHeader.svelte';
	import DashboardEmptyState from './DashboardEmptyState.svelte';

	interface ArtifactItem {
		id: string;
		name: string;
		item_type: 'file' | 'folder';
		updated_at: string;
		moduleKey: string;
		moduleName: string;
	}

	export let artifacts: ArtifactItem[];
	export let userName: string | undefined = undefined;
</script>

<section class="recent-artifacts" aria-label="Recent artifacts">
	<DashboardSectionHeader title="Recent artifacts" href="/files" />
	{#if artifacts.length === 0}
		<DashboardEmptyState
			title="No recent artifacts yet."
			description="Create a note, meeting record, decision, or board to start building your workspace memory."
		/>
	{:else}
		<ul class="artifact-list">
			{#each artifacts as item}
				{@const modColor = getModuleColor(item.moduleKey)}
				{@const ArtifactIcon = getArtifactIcon(item.moduleKey)}
				<li>
					<a href={getArtifactHref(item)} class="artifact-link">
						<div class="artifact-icon" style="background: {modColor.bg}; color: {modColor.color};">
							<ArtifactIcon size={16} />
						</div>
						<div class="artifact-body">
							<span class="artifact-name">{cleanArtifactName(item.name)}</span>
							<div class="artifact-meta">
								<span class="artifact-type-badge">{getArtifactTypeLabel(item.moduleKey, item.item_type)}</span>
								<span class="artifact-time">
									{formatDistanceToNow(new Date(item.updated_at), { addSuffix: true })}
								</span>
							</div>
						</div>
						<span class="artifact-user-avatar">
							{getUserInitials(userName)}
						</span>
					</a>
				</li>
			{/each}
		</ul>
		<div class="view-all-row">
			<a href="/files" class="view-all-btn">
				View all recent artifacts <ArrowUpRight size={14} />
			</a>
		</div>
	{/if}
</section>

<style>
	.artifact-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.artifact-link {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.65rem 0.85rem;
		border-radius: 0.75rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 35%, transparent);
		background: color-mix(in oklab, var(--base-100) 96%, white);
		color: inherit;
		text-decoration: none;
		transition:
			border-color 150ms ease,
			background 150ms ease;
	}
	.artifact-link:hover {
		border-color: color-mix(in oklab, var(--brand-500) 30%, transparent);
		background: color-mix(in oklab, var(--brand-500) 4%, white);
	}
	.artifact-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 0.5rem;
		flex-shrink: 0;
	}
	.artifact-body {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-width: 0;
		flex: 1;
	}
	.artifact-name {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--base-content);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.artifact-meta {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.72rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}
	.artifact-type-badge {
		padding: 0.1rem 0.45rem;
		border-radius: 999px;
		background: color-mix(in oklab, var(--base-300) 40%, transparent);
		font-size: 0.68rem;
		font-weight: 600;
		color: color-mix(in oklab, var(--base-content) 65%, transparent);
	}
	.artifact-time {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
	}
	.artifact-user-avatar {
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
	.view-all-row {
		display: flex;
		justify-content: center;
		padding-top: 0.75rem;
	}
	.view-all-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.82rem;
		font-weight: 600;
		color: var(--brand-500);
		text-decoration: none;
		transition: opacity 150ms ease;
	}
	.view-all-btn:hover {
		opacity: 0.8;
	}
</style>
