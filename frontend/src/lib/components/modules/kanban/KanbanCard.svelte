<script lang="ts">
	import { GripVertical, Paperclip, CheckSquare, Calendar } from 'lucide-svelte';
	import type { KanbanCard } from '$lib/api/types';

	interface Props {
		card: KanbanCard;
		onClick: () => void;
		onDragStart: (e: DragEvent) => void;
		onDragEnd: () => void;
	}

	let { card, onClick, onDragStart, onDragEnd }: Props = $props();

	function formatDate(dateStr: string) {
		const date = new Date(dateStr);
		const now = new Date();
		const isSameYear = date.getFullYear() === now.getFullYear();

		return new Intl.DateTimeFormat('en-US', {
			month: 'short',
			day: 'numeric',
			year: isSameYear ? undefined : 'numeric'
		}).format(date);
	}

	function isOverdue(dateStr: string) {
		const date = new Date(dateStr);
		const now = new Date();
		now.setHours(0, 0, 0, 0);
		return date < now;
	}
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

	{#if (card.labels && card.labels.length > 0) || card.priority !== 'normal'}
		<div class="card-labels">
			{#if card.priority !== 'normal'}
				<span class="card-label label-priority priority-{card.priority}">
					{card.priority}
				</span>
			{/if}
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
		<div class="card-badges">
			{#if card.attachments_count > 0}
				<span class="card-badge" title="Attachments">
					<Paperclip size={12} />
					{card.attachments_count}
				</span>
			{/if}
			{#if card.checklist.total > 0}
				<span
					class="card-badge"
					title="Checklist"
					class:badge-done={card.checklist.done === card.checklist.total}
				>
					<CheckSquare size={12} />
					{card.checklist.done}/{card.checklist.total}
				</span>
			{/if}
			{#if card.due_date}
				<span
					class="card-badge"
					title="Due Date"
					class:badge-overdue={isOverdue(card.due_date)}
				>
					<Calendar size={12} />
					{formatDate(card.due_date)}
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

	.priority-urgent {
		background: #eb5a46;
		color: white;
		box-shadow: 0 0 4px rgba(235, 90, 70, 0.4);
	}
	.priority-high {
		background: #ff9f1a;
		color: white;
	}
	.priority-low {
		background: #b3bac5;
		color: white;
	}

	.card-footer {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-top: 0.75rem;
		padding-left: 1.25rem;
		flex-wrap: wrap;
	}

	.card-badges {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		color: color-mix(in oklab, var(--base-content) 45%, transparent);
	}

	.card-badge {
		display: flex;
		align-items: center;
		gap: 0.2rem;
		font-size: 0.68rem;
		font-weight: 600;
	}

	.badge-done {
		color: #61bd4f;
	}

	.badge-overdue {
		color: #eb5a46;
		background: color-mix(in oklab, #eb5a46 8%, transparent);
		padding: 0 0.25rem;
		border-radius: 0.25rem;
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
