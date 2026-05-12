<script lang="ts">
	import { Folder, MoreVertical } from 'lucide-svelte';
	import DashboardSectionHeader from './DashboardSectionHeader.svelte';
	import DashboardEmptyState from './DashboardEmptyState.svelte';

	interface Folder {
		id: string;
		name: string;
		path: string;
	}

	export let folders: Folder[];
</script>

<section class="pinned-folders" aria-label="Pinned folders">
	<DashboardSectionHeader title="Pinned folders" href="/files" />
	{#if folders.length === 0}
		<DashboardEmptyState
			description="Star folders to pin them here for quick access."
			minimal
		/>
	{:else}
		<ul class="folder-list">
			{#each folders as folder}
				<li class="folder-item">
					<a href={`/files?folder=${folder.id}`} class="folder-link">
						<div class="folder-icon">
							<Folder size={18} />
						</div>
						<div class="folder-body">
							<span class="folder-name">{folder.name}</span>
							<span class="folder-path">{folder.path}</span>
						</div>
					</a>
					<button
						type="button"
						class="folder-menu-btn"
						aria-label="Folder options"
						onclick={(e) => e.preventDefault()}
					>
						<MoreVertical size={14} />
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.folder-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.folder-item {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.55rem 0.65rem;
		border-radius: 0.65rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 35%, transparent);
		background: color-mix(in oklab, var(--base-100) 96%, white);
		transition:
			border-color 150ms ease,
			background 150ms ease;
	}
	.folder-item:hover {
		border-color: color-mix(in oklab, var(--brand-500) 30%, transparent);
		background: color-mix(in oklab, var(--brand-500) 4%, white);
	}
	.folder-link {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		color: inherit;
		text-decoration: none;
		flex: 1;
		min-width: 0;
	}
	.folder-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 0.5rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 70%, white);
		color: color-mix(in oklab, var(--base-content) 55%, transparent);
		flex-shrink: 0;
	}
	.folder-body {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
		min-width: 0;
		flex: 1;
	}
	.folder-name {
		font-size: 0.82rem;
		font-weight: 600;
		color: var(--base-content);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.folder-path {
		font-size: 0.7rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.folder-menu-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 0.4rem;
		border: none;
		background: transparent;
		color: color-mix(in oklab, var(--base-content) 40%, transparent);
		cursor: pointer;
		flex-shrink: 0;
		transition: background 150ms ease, color 150ms ease;
	}
	.folder-menu-btn:hover {
		background: color-mix(in oklab, var(--base-300) 40%, transparent);
		color: var(--base-content);
	}
</style>
