<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { listBrainstormBoards } from '$lib/api/brainstorming';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { formatDistanceToNow } from 'date-fns';
	import { Plus, ImageOff, Lightbulb } from 'lucide-svelte';
	import { goto } from '$app/navigation';

	interface Props {
		module: ModuleDefinition;
	}

	let { module }: Props = $props();

	let widget = $derived(module.ui.dashboard.widget);

	const boardsQuery = createQuery({
		queryKey: ['brainstorm-boards', 'widget'],
		queryFn: () => listBrainstormBoards()
	});

	function getPreviewUrl(board: { preview_file_id: string | null }): string | null {
		if (!board.preview_file_id) return null;
		return `/api/v1/files/${board.preview_file_id}/content`;
	}

	function handleNewBoard() {
		goto('/modules/brainstorming');
	}
</script>

<div class="widget-card" data-size={widget.size}>
	<div class="widget-header">
		<div>
			<h3>{widget.title}</h3>
			<p>{widget.description}</p>
		</div>
		<span class="widget-chip">{module.rootPath}</span>
	</div>

	{#if $boardsQuery.isLoading}
		<div class="flex h-16 items-center justify-center">
			<div class="loading loading-sm loading-spinner text-brand-500"></div>
		</div>
	{:else if ($boardsQuery.data ?? []).length === 0}
		<div class="flex flex-col items-center gap-1.5 py-3 text-base-content/40">
			<Lightbulb size={18} />
			<p class="text-xs">No boards yet.</p>
		</div>
	{:else}
		<div class="board-grid">
			{#each $boardsQuery.data?.slice(0, widget.maxItems) ?? [] as board}
				<a href={`/modules/brainstorming/${board.id}`} class="board-item">
					<div class="board-thumb">
						{#if getPreviewUrl(board)}
							<img src={getPreviewUrl(board)!} alt={board.title} loading="lazy" />
						{:else}
							<div class="thumb-placeholder">
								<ImageOff size={14} />
							</div>
						{/if}
					</div>
					<div class="board-meta">
						<strong>{board.title}</strong>
						<span>
							{board.updated_at
								? formatDistanceToNow(new Date(board.updated_at), { addSuffix: true })
								: ''}
						</span>
					</div>
				</a>
			{/each}
		</div>
	{/if}

	{#if widget.primaryAction}
		<button class="widget-footer" onclick={handleNewBoard}>
			<span>{widget.primaryAction.label}</span>
			<Plus size={12} />
		</button>
	{/if}
</div>

<style>
	.widget-card {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		min-height: 100%;
		padding: 0.75rem;
		border-radius: 1rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 54%, transparent);
		background: color-mix(in oklab, var(--base-100) 94%, white);
		box-shadow: 0 8px 24px rgb(72 42 17 / 0.05);
	}

	.widget-card[data-size='small'] {
		min-height: 5.5rem;
	}

	.widget-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 0.5rem;
	}

	.widget-header h3 {
		margin: 0 0 0.15rem;
		font-size: 0.82rem;
		font-weight: 800;
		letter-spacing: -0.02em;
	}

	.widget-header p {
		margin: 0;
		font-size: 0.72rem;
		line-height: 1.4;
		color: color-mix(in oklab, var(--base-content) 64%, transparent);
	}

	.widget-chip {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.12rem 0.45rem;
		border-radius: 999px;
		border: 1px solid color-mix(in oklab, var(--base-300) 55%, transparent);
		background: var(--rs-surface-muted);
		font-size: 0.62rem;
		font-weight: 700;
		color: color-mix(in oklab, var(--base-content) 70%, transparent);
	}

	.board-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 0.5rem;
	}

	.board-item {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		border-radius: 0.65rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 40%, transparent);
		background: color-mix(in oklab, var(--rs-surface-muted) 58%, white);
		padding: 0.35rem;
		transition:
			border-color 150ms ease,
			background 150ms ease;
	}

	.board-item:hover {
		border-color: color-mix(in oklab, var(--brand-500) 35%, transparent);
		background: color-mix(in oklab, var(--rs-surface-muted) 40%, white);
	}

	.board-thumb {
		aspect-ratio: 16 / 10;
		border-radius: 0.5rem;
		overflow: hidden;
		background: var(--base-200);
	}

	.board-thumb img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.thumb-placeholder {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		height: 100%;
		color: color-mix(in oklab, var(--base-content) 30%, transparent);
	}

	.board-meta {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
		padding: 0 0.15rem;
	}

	.board-meta strong {
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--base-content);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.board-meta span {
		font-size: 0.65rem;
		color: color-mix(in oklab, var(--base-content) 55%, transparent);
	}

	.widget-footer {
		margin-top: 0.5rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.35rem;
		width: 100%;
		padding: 0.35rem;
		border-radius: 0.5rem;
		font-size: 0.75rem;
		font-weight: 700;
		color: var(--brand-500);
		background: transparent;
		border: 1px dashed color-mix(in oklab, var(--brand-500) 30%, transparent);
		cursor: pointer;
		transition: background 150ms ease;
	}

	.widget-footer:hover {
		background: color-mix(in oklab, var(--brand-500) 6%, transparent);
	}
</style>
