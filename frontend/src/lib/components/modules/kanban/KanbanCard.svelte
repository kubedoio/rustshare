<script lang="ts">
	import { GripVertical, Paperclip, CheckSquare } from 'lucide-svelte';
	import type { KanbanCard } from '$lib/api/types';

	interface Props {
		card: KanbanCard;
		onClick: () => void;
		onDragStart: (e: DragEvent) => void;
		onDragEnd: () => void;
	}

	let { card, onClick, onDragStart, onDragEnd }: Props = $props();
</script>

<div
	class="kanban-card"
	draggable="true"
	data-card-id={card.id}
	role="button"
	tabindex="0"
	ondragstart={onDragStart}
	ondragend={onDragEnd}
	onclick={onClick}
	onkeydown={(e) => {
		if (e.key === 'Enter' || e.key === ' ') onClick();
	}}
>
	<div class="kanban-card-title-row">
		<GripVertical size={14} class="text-base-content/30" />
		<strong>{card.title}</strong>
	</div>

	{#if card.description_preview}
		<div class="card-description">
			{card.description_preview}
		</div>
	{/if}

	{#if card.labels && card.labels.length > 0}
		<div class="card-labels">
			{#each card.labels.slice(0, 3) as label}
				<span class="card-label label-{label.color}">
					{label.name}
				</span>
			{/each}
			{#if card.labels.length > 3}
				<span class="card-label label-more">+{card.labels.length - 3}</span>
			{/if}
		</div>
	{/if}

	<div class="card-footer">
		<div class="card-meta-left">
			{#if card.priority && card.priority !== 'normal'}
				<span class="meta-badge priority-badge-small priority-{card.priority}">
					<span class="priority-dot-small"></span>
					{card.priority}
				</span>
			{/if}
			{#if card.attachments_count > 0}
				<span class="meta-badge">
					<Paperclip size={12} />
					{card.attachments_count}
				</span>
			{/if}
			{#if card.checklist && card.checklist.total > 0}
				<span class="meta-badge">
					<CheckSquare size={12} />
					<span>{card.checklist.done}/{card.checklist.total}</span>
				</span>
			{/if}
		</div>

		{#if card.assignees && card.assignees.length > 0}
			<div class="card-assignees">
				{#each card.assignees.slice(0, 3) as assignee}
					<div class="assignee-avatar" title={assignee.display_name}>
						{#if assignee.avatar_url}
							<img src={assignee.avatar_url} alt={assignee.display_name} />
						{:else}
							<span>{assignee.initials}</span>
						{/if}
					</div>
				{/each}
				{#if card.assignees.length > 3}
					<div class="assignee-avatar avatar-more" title="More assignees">
						<span>+{card.assignees.length - 3}</span>
					</div>
				{/if}
			</div>
		{/if}
	</div>
</div>

<style>
	.kanban-card {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		border-radius: 1rem;
		border: 1px solid rgba(133, 95, 44, 0.08);
		background: rgba(255, 255, 255, 0.85);
		padding: 0.75rem;
		cursor: pointer;
		text-align: left;
		box-shadow: 0 8px 18px rgb(72 42 17 / 0.05);
		transition:
			transform 160ms ease,
			border-color 160ms ease,
			box-shadow 160ms ease;
	}

	.kanban-card:hover {
		transform: translateY(-1px);
		border-color: color-mix(in oklab, var(--brand-500) 35%, transparent);
		box-shadow: 0 12px 24px rgb(72 42 17 / 0.08);
	}

	.kanban-card-title-row {
		display: flex;
		align-items: flex-start;
		gap: 0.4rem;
	}

	.kanban-card-title-row :global(svg) {
		margin-top: 0.15rem;
		flex-shrink: 0;
	}

	.card-description {
		margin-top: 0.15rem;
		padding-left: 1.25rem;
		font-size: 0.72rem;
		line-height: 1.35;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.card-labels {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
		margin-top: 0.5rem;
		padding-left: 1.25rem;
	}

	.card-label {
		font-size: 0.6rem;
		font-weight: 700;
		padding: 0.1rem 0.4rem;
		border-radius: 0.25rem;
		text-transform: uppercase;
		letter-spacing: 0.02em;
	}

	.label-green {
		background: #61bd4f;
		color: white;
	}
	.label-yellow {
		background: #f2d600;
		color: #42526e;
	}
	.label-orange {
		background: #ff9f1a;
		color: white;
	}
	.label-red {
		background: #eb5a46;
		color: white;
	}
	.label-purple {
		background: #c377e0;
		color: white;
	}
	.label-blue {
		background: #0079bf;
		color: white;
	}
	.label-gray {
		background: #b3bac5;
		color: white;
	}
	.label-more {
		background: var(--base-200);
		color: var(--base-content);
		opacity: 0.8;
	}

	.card-footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		margin-top: 0.75rem;
		padding-left: 1.25rem;
		flex-wrap: wrap;
	}

	.card-meta-left {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		flex-wrap: wrap;
	}

	.meta-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.15rem 0.4rem;
		border-radius: 0.35rem;
		background: color-mix(in oklab, var(--base-200) 60%, transparent);
		font-size: 0.65rem;
		font-weight: 700;
		color: color-mix(in oklab, var(--base-content) 70%, transparent);
		text-transform: capitalize;
	}

	.priority-badge-small {
		text-transform: capitalize;
	}

	.priority-dot-small {
		width: 0.35rem;
		height: 0.35rem;
		border-radius: 999px;
	}

	.priority-low {
		background: #eff6ff;
		color: #2563eb;
	}
	.priority-low .priority-dot-small {
		background: #3b82f6;
	}

	.priority-high {
		background: #fff7ed;
		color: #ea580c;
	}
	.priority-high .priority-dot-small {
		background: #f97316;
	}

	.priority-urgent {
		background: #fef2f2;
		color: #dc2626;
	}
	.priority-urgent .priority-dot-small {
		background: #ef4444;
	}

	.card-assignees {
		display: flex;
	}

	.assignee-avatar {
		width: 1.4rem;
		height: 1.4rem;
		border-radius: 999px;
		background: var(--base-200);
		border: 1.5px solid var(--rs-surface-primary, white);
		margin-right: -0.3rem;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 0.55rem;
		font-weight: 700;
		color: var(--base-content);
		overflow: hidden;
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
	}

	.assignee-avatar img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.avatar-more {
		background: var(--base-300);
		color: var(--base-content);
	}

	.kanban-card strong {
		font-size: 0.9rem;
		font-weight: 700;
		color: var(--base-content);
		line-height: 1.4;
	}
</style>
