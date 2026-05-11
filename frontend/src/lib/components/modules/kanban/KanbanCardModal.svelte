<script lang="ts">
	import { X, Plus, Check, AlignLeft, ChevronDown, Trash2 } from 'lucide-svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import type { KanbanCardDetail, KanbanBoard, KanbanAssignee } from '$lib/api/types';
	import RichMarkdownEditor from '../../../editor/components/RichMarkdownEditor.svelte';

	interface Props {
		card: KanbanCardDetail | null;
		open: boolean;
		title: string;
		board: KanbanBoard;
		assignableUsers: KanbanAssignee[];
		loadingDetail: boolean;
		savingDetail: boolean;
		saveStatus: 'idle' | 'saving' | 'saved' | 'error';
		onClose: () => void;
		onSave: () => void;
		onArchive: () => void;
		onDelete: () => void;
		onToggleLabel: (labelId: string) => void;
		onToggleAssignee: (userId: string) => void;
		onCreateLabel: (name: string, color: string) => void;
	}

	let {
		card,
		open,
		title,
		board,
		assignableUsers,
		loadingDetail,
		savingDetail,
		saveStatus,
		onClose,
		onSave,
		onArchive,
		onDelete,
		onToggleLabel,
		onToggleAssignee
	}: Props = $props();

	let showLabelPicker = $state(false);
	let showAssigneePicker = $state(false);

	let selectedAssignee = $derived(card?.assignees?.[0] ?? null);

	function handleAssigneeSelect(userId: string) {
		if (!card) return;
		for (const assignee of card.assignees) {
			if (assignee.id !== userId) onToggleAssignee(assignee.id);
		}
		if (!card.assignees.some((assignee) => assignee.id === userId)) {
			onToggleAssignee(userId);
		}
		showAssigneePicker = false;
	}
</script>

<ModalBase {open} {onClose} {title} class="max-w-3xl !overflow-visible">
	<div class="card-detail-drawer">
		{#if loadingDetail}
			<div class="flex h-64 items-center justify-center">
				<span class="loading loading-lg loading-spinner text-brand-500"></span>
			</div>
		{:else if card}
			<header class="detail-header">
				<div class="header-main">
					<div class="title-row">
						<input
							type="text"
							bind:value={card.title}
							class="detail-title-input"
							placeholder="Card Title"
							onblur={onSave}
						/>
					</div>
					<div class="detail-meta">in column <span class="font-bold">{card.status}</span></div>
				</div>
				<div class="header-actions">
					{#if saveStatus === 'saving'}
						<span class="text-xs text-base-content/50">Saving...</span>
					{:else if saveStatus === 'saved'}
						<span class="text-xs text-green-600">Saved</span>
					{:else if saveStatus === 'error'}
						<span class="text-xs text-red-600">Error saving</span>
					{/if}
					<button class="btn-close" onclick={onClose}>
						<X size={20} />
					</button>
				</div>
			</header>

			<div class="detail-content">
				<div class="detail-main">
					<div class="detail-section property-section">
						<h4 class="section-label">Label</h4>
						<div class="property-anchor">
							<div class="label-strip">
								{#each card.labels as label}
									<span class="card-label label-{label.color}">
										{label.name}
									</span>
								{/each}
								<button
									class="property-add-button"
									aria-label="Add label"
									onclick={() => {
										showLabelPicker = !showLabelPicker;
										showAssigneePicker = false;
									}}
								>
									<Plus size={16} />
								</button>
							</div>
							{#if showLabelPicker}
								<div class="property-menu label-menu">
									{#if board.labels.length === 0}
										<div class="property-menu-empty">No labels on this board</div>
									{:else}
										{#each board.labels as label}
											<button class="property-menu-item" onclick={() => onToggleLabel(label.id)}>
												<span class="card-label label-{label.color}">{label.name}</span>
												{#if card.labels.some((selected) => selected.id === label.id)}
													<Check size={14} class="text-brand-600" />
												{/if}
											</button>
										{/each}
									{/if}
								</div>
							{/if}
						</div>
					</div>

					<div class="detail-section property-section">
						<h4 class="section-label">Assignee</h4>
						<div class="property-anchor">
							<button
								class="assignee-select"
								onclick={() => {
									showAssigneePicker = !showAssigneePicker;
									showLabelPicker = false;
								}}
							>
								{#if selectedAssignee}
									<span class="assignee-avatar">
										{#if selectedAssignee.avatar_url}
											<img src={selectedAssignee.avatar_url} alt={selectedAssignee.display_name} />
										{:else}
											<span>{selectedAssignee.initials}</span>
										{/if}
									</span>
									<span>{selectedAssignee.display_name}</span>
								{:else}
									<span class="assignee-avatar empty-avatar">
										<Plus size={13} />
									</span>
									<span>Choose assignee</span>
								{/if}
								<ChevronDown size={16} class="ml-auto text-base-content/45" />
							</button>
							{#if showAssigneePicker}
								<div class="property-menu assignee-menu">
									{#if assignableUsers.length === 0}
										<div class="property-menu-empty">No assignable members</div>
									{:else}
										{#each assignableUsers as user}
											<button class="property-menu-item" onclick={() => handleAssigneeSelect(user.id)}>
												<span class="assignee-avatar">
													{#if user.avatar_url}
														<img src={user.avatar_url} alt={user.display_name} />
													{:else}
														<span>{user.initials}</span>
													{/if}
												</span>
												<span class="flex-1 truncate">{user.display_name}</span>
												{#if card.assignees.some((assignee) => assignee.id === user.id)}
													<Check size={14} class="text-brand-600" />
												{/if}
											</button>
										{/each}
									{/if}
								</div>
							{/if}
						</div>
					</div>

					<div class="detail-section">
						<div class="mb-2 flex items-center gap-2">
							<AlignLeft size={18} class="text-base-content/60" />
							<h4 class="section-label !mb-0">Description</h4>
						</div>
						<div class="description-editor">
							<RichMarkdownEditor
								content={card.content}
								bind:currentMarkdown={card.content}
								editable={true}
								on:change={() => {
									// saveStatus is managed by parent via saveStatus prop
								}}
								hasAttachmentHandler={false}
							/>
							<div class="mt-2 flex justify-end">
								<button
									class="btn btn-sm btn-primary"
									disabled={savingDetail}
									onclick={onSave}
								>
									{#if savingDetail}
										<span class="loading loading-xs loading-spinner"></span>
									{/if}
									Save changes
								</button>
							</div>
						</div>
					</div>

					<div class="detail-section flex items-center justify-between gap-2">
						<button class="delete-icon-button" aria-label="Delete card" onclick={onDelete}>
							<Trash2 size={17} />
						</button>
						<button class="btn btn-sm btn-outline" onclick={onArchive}>Archive</button>
					</div>
				</div>
			</div>
		{/if}
	</div>
</ModalBase>

<style>
	.card-detail-drawer {
		width: 100%;
		max-width: 48rem;
		min-height: 30rem;
		max-height: 90vh;
		overflow-y: visible;
		background: var(--rs-surface-primary, white);
		display: flex;
		flex-direction: column;
		border-radius: 0.75rem;
	}

	.detail-header {
		display: flex;
		justify-content: space-between;
		padding: 1.5rem;
		background: color-mix(in oklab, var(--base-100) 95%, black);
		border-bottom: 1px solid var(--base-200);
		position: sticky;
		top: 0;
		z-index: 10;
	}

	.detail-title-input {
		font-size: 1.5rem;
		font-weight: 800;
		color: var(--base-content);
		background: transparent;
		border: 1px solid transparent;
		width: 100%;
		border-radius: 0.5rem;
		padding: 0.25rem 0.5rem;
		margin-left: -0.5rem;
		transition: all 0.2s;
	}

	.detail-title-input:focus {
		background: white;
		border-color: var(--brand-500);
		outline: none;
		box-shadow: 0 0 0 3px color-mix(in oklab, var(--brand-500) 15%, transparent);
	}

	.detail-meta {
		font-size: 0.85rem;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
		margin-top: 0.25rem;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	.detail-content {
		padding: 1.5rem;
		display: grid;
		grid-template-columns: 1fr;
		gap: 2rem;
		overflow: visible;
	}

	.detail-section {
		margin-bottom: 1.5rem;
	}

	.property-section {
		border-bottom: 1px solid var(--base-200);
		padding-bottom: 1rem;
		overflow: visible;
	}

	.section-label {
		font-size: 0.85rem;
		font-weight: 700;
		color: color-mix(in oklab, var(--base-content) 70%, transparent);
		text-transform: none;
		letter-spacing: 0;
		margin-bottom: 0.75rem;
	}

	.property-anchor {
		position: relative;
		width: 100%;
		overflow: visible;
	}

	.label-strip {
		display: flex;
		min-height: 2.25rem;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
	}

	.property-add-button {
		display: inline-flex;
		width: 2rem;
		height: 2rem;
		flex-shrink: 0;
		align-items: center;
		justify-content: center;
		border-radius: 0.65rem;
		border: 1px solid var(--base-200);
		background: var(--base-100, #ffffff);
		color: color-mix(in oklab, var(--base-content) 64%, transparent);
	}

	.property-add-button:hover,
	.assignee-select:hover {
		border-color: color-mix(in oklab, var(--brand-500) 38%, transparent);
		background: color-mix(in oklab, var(--brand-500) 5%, white);
	}

	.property-menu {
		position: absolute;
		top: calc(100% + 0.5rem);
		left: 0;
		z-index: 80;
		width: min(22rem, 100%);
		min-width: 16rem;
		max-height: 16rem;
		overflow-y: auto;
		border-radius: 0.75rem;
		border: 1px solid color-mix(in oklab, var(--base-content) 12%, transparent);
		background: var(--base-100, #ffffff);
		padding: 0.4rem;
		box-shadow:
			0 24px 55px rgb(15 23 42 / 0.24),
			0 0 0 1px rgb(255 255 255 / 0.92) inset;
		color: var(--base-content);
	}

	.label-menu {
		width: min(20rem, 100%);
	}

	.assignee-menu {
		width: 100%;
		min-width: 18rem;
	}

	.property-menu-item {
		display: flex;
		width: 100%;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		border-radius: 0.6rem;
		background: transparent;
		padding: 0.5rem 0.6rem;
		text-align: left;
		font-size: 0.82rem;
		color: var(--base-content);
	}

	.property-menu-item:hover {
		background: color-mix(in oklab, var(--brand-500) 8%, var(--base-100, #ffffff));
	}

	.property-menu-empty {
		padding: 0.65rem 0.75rem;
		color: color-mix(in oklab, var(--base-content) 55%, transparent);
		font-size: 0.82rem;
	}

	.card-label {
		display: inline-flex;
		align-items: center;
		min-height: 1.45rem;
		max-width: 100%;
		border-radius: 0.35rem;
		padding: 0.18rem 0.45rem;
		font-size: 0.68rem;
		font-weight: 800;
		line-height: 1;
		text-transform: uppercase;
		letter-spacing: 0.02em;
	}

	.label-green {
		background: #d8f2dc;
		color: #188039;
	}
	.label-yellow {
		background: #fff1a8;
		color: #826300;
	}
	.label-orange {
		background: #ffe2bd;
		color: #9a5200;
	}
	.label-red {
		background: #ffd8d8;
		color: #c02b2b;
	}
	.label-purple {
		background: #eadcff;
		color: #6941b8;
	}
	.label-blue {
		background: #dce9ff;
		color: #2f62b6;
	}
	.label-gray {
		background: var(--base-200);
		color: color-mix(in oklab, var(--base-content) 70%, transparent);
	}

	.assignee-select {
		display: flex;
		width: 100%;
		align-items: center;
		gap: 0.55rem;
		border-radius: 0.75rem;
		border: 1px solid var(--base-200);
		background: var(--base-100, #ffffff);
		padding: 0.55rem 0.65rem;
		font-size: 0.85rem;
		font-weight: 600;
		text-align: left;
		color: var(--base-content);
	}

	.assignee-avatar {
		display: inline-flex;
		width: 1.65rem;
		height: 1.65rem;
		flex-shrink: 0;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		border-radius: 999px;
		background: color-mix(in oklab, var(--brand-500) 14%, white);
		color: var(--brand-700, #8b3d12);
		font-size: 0.68rem;
		font-weight: 800;
	}

	.assignee-avatar img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.empty-avatar {
		background: var(--base-200);
		color: color-mix(in oklab, var(--base-content) 55%, transparent);
	}

	.description-editor {
		background: color-mix(in oklab, var(--rs-surface-muted) 30%, white);
		border-radius: 0.75rem;
		border: 1px solid var(--base-200);
		padding: 0.5rem;
	}

	.btn-close {
		padding: 0.5rem;
		border-radius: 999px;
		color: var(--base-content);
		opacity: 0.5;
		transition: all 0.2s;
	}

	.btn-close:hover {
		background: var(--base-200);
		opacity: 1;
	}

	.delete-icon-button {
		display: inline-flex;
		width: 2.25rem;
		height: 2.25rem;
		align-items: center;
		justify-content: center;
		border-radius: 0.65rem;
		border: 1px solid color-mix(in oklab, var(--error) 25%, transparent);
		background: color-mix(in oklab, var(--error) 6%, white);
		color: var(--error);
	}

	.delete-icon-button:hover {
		background: color-mix(in oklab, var(--error) 10%, white);
	}
</style>
