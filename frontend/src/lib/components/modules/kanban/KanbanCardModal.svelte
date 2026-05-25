<script lang="ts">
	import {
		X,
		Plus,
		Check,
		AlignLeft,
		ChevronDown,
		Trash2,
		Layout,
		Paperclip,
		CheckSquare,
		MessageSquare,
		Send,
		Share2,
		XCircle
	} from 'lucide-svelte';
	import { get } from 'svelte/store';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import RichMarkdownEditor from '../../../editor/components/RichMarkdownEditor.svelte';
	import type { KanbanCardDetail, KanbanBoard, KanbanAssignee, KanbanEvent } from '$lib/api/types';
	import { addCardAttachment, deleteCardAttachment } from '$lib/api/kanban';
	import { currentUser } from '$lib/stores/auth';

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
		onToggleAssignee,
		onCreateLabel
	}: Props = $props();

	let showLabelPicker = $state(false);
	let showAssigneePicker = $state(false);
	let newChecklistItemText = $state('');
	let newCommentText = $state('');
	let isDraggingFiles = $state(false);
	let fileInputRef: HTMLInputElement | undefined = $state();

	let selectedAssignee = $derived(card?.assignees?.[0] ?? null);

	let checklistStats = $derived(
		card && card.checklists
			? card.checklists.reduce(
					(acc, group) => {
						for (const item of group.items) {
							acc.total++;
							if (item.done) acc.done++;
						}
						return acc;
					},
					{ done: 0, total: 0 }
				)
			: { done: 0, total: 0 }
	);
	let checklistPercent = $derived(
		checklistStats.total > 0 ? Math.round((checklistStats.done / checklistStats.total) * 100) : 0
	);

	function priorityBadge(priority: string) {
		switch (priority) {
			case 'low':
				return { label: 'Low', dot: 'bg-blue-400', text: 'text-blue-600', bg: 'bg-blue-50' };
			case 'high':
				return { label: 'High', dot: 'bg-orange-400', text: 'text-orange-600', bg: 'bg-orange-50' };
			case 'urgent':
				return { label: 'Urgent', dot: 'bg-red-500', text: 'text-red-600', bg: 'bg-red-50' };
			default:
				return null;
		}
	}

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

	function toggleChecklistItem(groupId: string, itemId: string) {
		if (!card) return;
		card.checklists = card.checklists.map((g) => {
			if (g.id !== groupId) return g;
			return {
				...g,
				items: g.items.map((i) => (i.id === itemId ? { ...i, done: !i.done } : i))
			};
		});
		const stats = card.checklists.reduce(
			(acc, g) => {
				for (const item of g.items) {
					acc.total++;
					if (item.done) acc.done++;
				}
				return acc;
			},
			{ done: 0, total: 0 }
		);
		card.checklist = { done: stats.done, total: stats.total };
	}

	function addChecklistItem() {
		if (!card || !newChecklistItemText.trim()) return;
		const text = newChecklistItemText.trim();
		if (card.checklists.length === 0) {
			card.checklists = [
				{
					id: `new-${Date.now()}`,
					title: 'Checklist',
					items: [{ id: `new-item-${Date.now()}`, text, done: false }]
				}
			];
		} else {
			card.checklists = card.checklists.map((g, i) =>
				i === 0
					? { ...g, items: [...g.items, { id: `new-item-${Date.now()}`, text, done: false }] }
					: g
			);
		}
		newChecklistItemText = '';
		const stats = card.checklists.reduce(
			(acc, g) => {
				for (const item of g.items) {
					acc.total++;
					if (item.done) acc.done++;
				}
				return acc;
			},
			{ done: 0, total: 0 }
		);
		card.checklist = { done: stats.done, total: stats.total };
	}

	async function removeAttachment(attachmentId: string) {
		if (!card) return;
		try {
			await deleteCardAttachment(card.id, attachmentId);
			card.attachments = card.attachments.filter((a) => a.id !== attachmentId);
			card.attachments_count = card.attachments.length;
		} catch (err) {
			console.error('Failed to delete attachment:', err);
		}
	}

	async function handleFileDrop(e: DragEvent) {
		e.preventDefault();
		isDraggingFiles = false;
		const files = e.dataTransfer?.files;
		if (files && files.length > 0 && card) {
			for (const file of Array.from(files)) {
				try {
					const attachment = await addCardAttachment(card.id, file);
					card.attachments = [...(card.attachments || []), attachment];
					card.attachments_count = card.attachments.length;
				} catch (err) {
					console.error('Failed to upload attachment:', err);
				}
			}
		}
	}

	async function handleFileSelect(e: Event) {
		const input = e.target as HTMLInputElement;
		const files = input.files;
		if (files && files.length > 0 && card) {
			for (const file of Array.from(files)) {
				try {
					const attachment = await addCardAttachment(card.id, file);
					card.attachments = [...(card.attachments || []), attachment];
					card.attachments_count = card.attachments.length;
				} catch (err) {
					console.error('Failed to upload attachment:', err);
				}
			}
		}
		if (input) input.value = '';
	}

	function submitComment() {
		if (!card || !newCommentText.trim()) return;
		const user = get(currentUser);
		const event: KanbanEvent = {
			event_type: 'comment',
			timestamp: new Date().toISOString(),
			actor: user?.display_name || 'You',
			payload: { text: newCommentText.trim() }
		};
		card.activity = [...card.activity, event];
		newCommentText = '';
	}

	function formatEventText(event: KanbanEvent): string {
		switch (event.event_type) {
			case 'comment':
				return 'commented';
			case 'created':
				return 'created this card';
			case 'moved':
				return 'moved this card';
			case 'assigned':
				return 'changed assignees';
			case 'label_added':
				return 'added a label';
			case 'label_removed':
				return 'removed a label';
			default:
				return event.event_type.replace(/_/g, ' ');
		}
	}

	function initialsFromName(name: string): string {
		return name
			.split(' ')
			.map((n) => n[0])
			.join('')
			.toUpperCase()
			.slice(0, 2);
	}

	function formatFileSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}
</script>

<ModalBase {open} {onClose} {title} class="max-w-3xl !overflow-visible" showCloseButton={false}>
	<div class="card-detail-drawer">
		{#if loadingDetail}
			<div class="flex h-64 items-center justify-center">
				<span class="loading loading-lg loading-spinner text-brand-500"></span>
			</div>
		{:else if card}
			<!-- Header -->
			<header class="detail-header">
				<div class="header-main">
					<div class="title-row">
						<Layout size={20} class="text-base-content/50 mt-1.5 flex-shrink-0" />
						<input
							type="text"
							bind:value={card.title}
							class="detail-title-input"
							placeholder="Card Title"
						/>
					</div>
					<div class="detail-meta">
						<span>in column <strong>{card.status}</strong></span>
						{#if priorityBadge(card.priority)}
							{@const badge = priorityBadge(card.priority)}
							<span class="priority-badge {badge?.bg}">
								<span class="priority-dot {badge?.dot}"></span>
								<span class={badge?.text}>{badge?.label}</span>
							</span>
						{/if}
					</div>
				</div>
				<div class="header-actions">
					{#if selectedAssignee}
						<button
							class="assignee-chip"
							onclick={() => {
								showAssigneePicker = !showAssigneePicker;
								showLabelPicker = false;
							}}
						>
							<span class="assignee-avatar small">
								{#if selectedAssignee.avatar_url}
									<img src={selectedAssignee.avatar_url} alt={selectedAssignee.display_name} />
								{:else}
									<span>{selectedAssignee.initials}</span>
								{/if}
							</span>
							<span class="text-sm font-medium">{selectedAssignee.display_name}</span>
							<ChevronDown size={14} class="text-base-content/40" />
						</button>
					{:else}
						<button
							class="assignee-chip empty"
							onclick={() => {
								showAssigneePicker = !showAssigneePicker;
								showLabelPicker = false;
							}}
						>
							<span class="assignee-avatar small empty-avatar">
								<Plus size={12} />
							</span>
							<span class="text-sm text-base-content/60">Assign</span>
							<ChevronDown size={14} class="text-base-content/40" />
						</button>
					{/if}
					{#if showAssigneePicker}
						<div class="property-menu assignee-menu">
							{#if assignableUsers.length === 0}
								<div class="property-menu-empty">No assignable members</div>
							{:else}
								{#each assignableUsers as user}
									<button class="property-menu-item" onclick={() => handleAssigneeSelect(user.id)}>
										<span class="assignee-avatar small">
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

					<button class="btn-close" onclick={onClose} aria-label="Close">
						<X size={20} />
					</button>
				</div>
			</header>

			<div class="detail-content">
				<!-- Labels -->
				<div class="detail-section property-section">
					<div class="label-strip">
						{#each card.labels as label}
							<span class="card-label label-{label.color}">{label.name}</span>
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

				<!-- Description -->
				<div class="detail-section">
					<div class="section-header">
						<AlignLeft size={18} class="text-base-content/60" />
						<h4 class="section-label !mb-0">Description</h4>
					</div>
					<div class="description-editor">
						<RichMarkdownEditor
							content={card.content}
							bind:currentMarkdown={card.content}
							editable={true}
							on:change={() => {}}
							hasAttachmentHandler={false}
						/>
					</div>
				</div>

				<!-- Attachments -->
				<div class="detail-section">
					<div class="section-header">
						<Paperclip size={18} class="text-base-content/60" />
						<h4 class="section-label !mb-0">Attachments</h4>
					</div>
					<div
						class="attachment-dropzone"
						class:dragging={isDraggingFiles}
						ondragover={(e) => {
							e.preventDefault();
							isDraggingFiles = true;
						}}
						ondragleave={() => (isDraggingFiles = false)}
						ondrop={handleFileDrop}
						onclick={() => fileInputRef?.click()}
						onkeydown={(e) => {
							if (e.key === 'Enter' || e.key === ' ') fileInputRef?.click();
						}}
						role="button"
						tabindex="0"
					>
						<input
							type="file"
							multiple
							class="hidden"
							bind:this={fileInputRef}
							onchange={handleFileSelect}
						/>
						<Paperclip size={16} class="text-base-content/40" />
						<span class="text-sm text-base-content/50">Add attachments or drop files here</span>
					</div>
					{#if card.attachments && card.attachments.length > 0}
						<div class="attachment-list">
							{#each card.attachments as attachment, i}
								<div class="attachment-item">
									<div class="attachment-info">
										<span class="attachment-name">{attachment.name}</span>
										<span class="attachment-meta">{formatFileSize(attachment.size)}</span>
									</div>
									<button
										class="attachment-remove"
										aria-label="Remove attachment"
										onclick={() => removeAttachment(attachment.id)}
									>
										<XCircle size={16} />
									</button>
								</div>
							{/each}
						</div>
					{/if}
				</div>

				<!-- Checklist -->
				<div class="detail-section">
					<div class="section-header">
						<CheckSquare size={18} class="text-base-content/60" />
						<h4 class="section-label !mb-0">
							Checklist — {checklistStats.done} of {checklistStats.total}
							{#if checklistStats.total > 0}({checklistPercent}%){/if}
						</h4>
					</div>
					{#if checklistStats.total > 0}
						<progress
							class="progress progress-primary w-full mb-3"
							value={checklistPercent}
							max="100"
						></progress>
					{/if}
					<div class="checklist-body">
						{#each card.checklists as group}
							{#if card.checklists.length > 1}
								<div class="checklist-group-title">{group.title}</div>
							{/if}
							{#each group.items as item}
								<label class="checklist-item">
									<input
										type="checkbox"
										checked={item.done}
										onchange={() => toggleChecklistItem(group.id, item.id)}
									/>
									<span class="checklist-text" class:done={item.done}>{item.text}</span>
								</label>
							{/each}
						{/each}
					</div>
					<div class="checklist-add">
						<input
							type="text"
							class="checklist-add-input"
							placeholder="+ Add an item"
							bind:value={newChecklistItemText}
							onkeydown={(e) => {
								if (e.key === 'Enter') {
									e.preventDefault();
									addChecklistItem();
								}
							}}
							onblur={addChecklistItem}
						/>
					</div>
				</div>

				<!-- Activity -->
				<div class="detail-section">
					<div class="section-header">
						<MessageSquare size={18} class="text-base-content/60" />
						<h4 class="section-label !mb-0">Activity</h4>
					</div>
					<div class="activity-list">
						{#each card.activity as event}
							<div class="activity-item">
								<div class="activity-avatar">
									<span>{initialsFromName(event.actor)}</span>
								</div>
								<div class="activity-body">
									<div class="activity-line">
										<strong>{event.actor}</strong>
										<span class="text-base-content/60">{formatEventText(event)}</span>
									</div>
									{#if event.event_type === 'comment'}
										{@const commentPayload = event.payload as { text?: string } | undefined}
										{#if commentPayload?.text}
											<div class="activity-comment">{commentPayload.text}</div>
										{/if}
									{/if}
									<div class="activity-time">
										{new Date(event.timestamp).toLocaleString(undefined, {
											month: 'short',
											day: 'numeric',
											hour: '2-digit',
											minute: '2-digit'
										})}
									</div>
								</div>
							</div>
						{/each}
					</div>
					<div class="comment-input-row">
						<div class="activity-avatar self-start">
							{#if $currentUser?.avatar_path}
								<img src={$currentUser.avatar_path} alt={$currentUser.display_name} />
							{:else}
								<span>{initialsFromName($currentUser?.display_name || 'You')}</span>
							{/if}
						</div>
						<div class="comment-box">
							<textarea
								class="comment-textarea"
								placeholder="Write a comment..."
								rows={2}
								bind:value={newCommentText}
								onkeydown={(e) => {
									if (e.key === 'Enter' && !e.shiftKey) {
										e.preventDefault();
										submitComment();
									}
								}}
							></textarea>
							<div class="comment-actions">
								<button
									class="btn btn-sm btn-primary gap-1"
									onclick={submitComment}
									disabled={!newCommentText.trim()}
								>
									<Send size={14} />
									Comment
								</button>
							</div>
						</div>
					</div>
				</div>
			</div>

			<!-- Footer -->
			<footer class="detail-footer">
				<div class="footer-left">
					<button class="btn btn-sm btn-ghost gap-1" onclick={() => console.log('share')}>
						<Share2 size={14} />
						Share
					</button>
					<button class="btn btn-sm btn-outline" onclick={onArchive}>Archive</button>
					<button class="btn btn-sm btn-outline btn-error" onclick={onDelete}>Delete</button>
				</div>
				<div class="footer-right">
					{#if saveStatus === 'saving'}
						<span class="text-xs text-base-content/50">Saving...</span>
					{:else if saveStatus === 'saved'}
						<span class="text-xs text-green-600">Saved</span>
					{:else if saveStatus === 'error'}
						<span class="text-xs text-red-600">Error saving</span>
					{/if}
					<button class="btn btn-sm btn-ghost" onclick={onClose}>Cancel</button>
					<button class="btn btn-sm btn-primary" disabled={savingDetail} onclick={onSave}>
						{#if savingDetail}
							<span class="loading loading-xs loading-spinner"></span>
						{/if}
						Save changes
					</button>
				</div>
			</footer>
		{/if}
	</div>
</ModalBase>

<style>
	.card-detail-drawer {
		width: 100%;
		max-width: 48rem;
		min-height: 30rem;
		max-height: 90vh;
		overflow-y: auto;
		background: var(--rs-surface-primary, white);
		display: flex;
		flex-direction: column;
		border-radius: 0.75rem;
	}

	.detail-header {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		padding: 1.25rem 1.5rem;
		background: color-mix(in oklab, var(--base-100) 95%, black);
		border-bottom: 1px solid var(--base-200);
		position: sticky;
		top: 0;
		z-index: 10;
		gap: 1rem;
	}

	.header-main {
		flex: 1;
		min-width: 0;
	}

	.title-row {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
	}

	.detail-title-input {
		font-size: 1.35rem;
		font-weight: 700;
		color: var(--base-content);
		background: transparent;
		border: 1px solid transparent;
		width: 100%;
		border-radius: 0.5rem;
		padding: 0.25rem 0.5rem;
		margin-left: -0.5rem;
		transition: all 0.2s;
		line-height: 1.3;
	}

	.detail-title-input:focus {
		background: white;
		border-color: var(--brand-500);
		outline: none;
		box-shadow: 0 0 0 3px color-mix(in oklab, var(--brand-500) 15%, transparent);
	}

	.detail-meta {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		font-size: 0.8rem;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
		margin-top: 0.35rem;
		padding-left: 1.75rem;
		flex-wrap: wrap;
	}

	.priority-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.15rem 0.5rem;
		border-radius: 999px;
		font-size: 0.7rem;
		font-weight: 700;
	}

	.priority-dot {
		width: 0.45rem;
		height: 0.45rem;
		border-radius: 999px;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		position: relative;
		flex-shrink: 0;
	}

	.assignee-chip {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.3rem 0.6rem;
		border-radius: 0.65rem;
		border: 1px solid var(--base-200);
		background: var(--base-100, white);
		transition: all 0.2s;
	}

	.assignee-chip:hover {
		border-color: color-mix(in oklab, var(--brand-500) 38%, transparent);
		background: color-mix(in oklab, var(--brand-500) 5%, white);
	}

	.assignee-chip.empty {
		background: transparent;
	}

	.btn-close {
		padding: 0.5rem;
		border-radius: 999px;
		color: var(--base-content);
		opacity: 0.5;
		transition: all 0.2s;
		flex-shrink: 0;
	}

	.btn-close:hover {
		background: var(--base-200);
		opacity: 1;
	}

	.detail-content {
		padding: 1.5rem;
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
		flex: 1;
	}

	.property-section {
		border-bottom: 1px solid var(--base-200);
		padding-bottom: 1rem;
		position: relative;
	}

	.section-header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.75rem;
	}

	.section-label {
		font-size: 0.85rem;
		font-weight: 700;
		color: color-mix(in oklab, var(--base-content) 70%, transparent);
		text-transform: none;
		letter-spacing: 0;
		margin-bottom: 0.75rem;
	}

	.label-strip {
		display: flex;
		min-height: 2rem;
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

	.property-add-button:hover {
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
		left: auto;
		right: 0;
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

	.assignee-avatar.small {
		width: 1.4rem;
		height: 1.4rem;
		font-size: 0.6rem;
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

	.attachment-dropzone {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		padding: 0.875rem;
		border: 2px dashed var(--base-300);
		border-radius: 0.75rem;
		background: color-mix(in oklab, var(--base-100) 80%, transparent);
		cursor: pointer;
		transition: all 0.2s;
	}

	.attachment-dropzone:hover,
	.attachment-dropzone.dragging {
		border-color: var(--brand-500);
		background: color-mix(in oklab, var(--brand-500) 5%, white);
	}

	.attachment-list {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		margin-top: 0.75rem;
	}

	.attachment-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		padding: 0.5rem 0.75rem;
		border-radius: 0.6rem;
		border: 1px solid var(--base-200);
		background: var(--base-100);
	}

	.attachment-info {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		min-width: 0;
	}

	.attachment-name {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--base-content);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.attachment-meta {
		font-size: 0.7rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}

	.attachment-remove {
		color: color-mix(in oklab, var(--base-content) 40%, transparent);
		flex-shrink: 0;
		padding: 0.25rem;
		border-radius: 999px;
		transition: all 0.2s;
	}

	.attachment-remove:hover {
		color: var(--error);
		background: color-mix(in oklab, var(--error) 8%, white);
	}

	.checklist-body {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.checklist-group-title {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--base-content);
		margin: 0.5rem 0 0.25rem;
	}

	.checklist-item {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.4rem 0.5rem;
		border-radius: 0.5rem;
		cursor: pointer;
		transition: background 0.15s;
	}

	.checklist-item:hover {
		background: color-mix(in oklab, var(--base-200) 50%, transparent);
	}

	.checklist-item input[type='checkbox'] {
		width: 1rem;
		height: 1rem;
		accent-color: var(--brand-500);
		cursor: pointer;
		flex-shrink: 0;
	}

	.checklist-text {
		font-size: 0.85rem;
		color: var(--base-content);
		flex: 1;
	}

	.checklist-text.done {
		text-decoration: line-through;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}

	.checklist-add {
		margin-top: 0.5rem;
	}

	.checklist-add-input {
		width: 100%;
		padding: 0.5rem 0.75rem;
		border-radius: 0.5rem;
		border: 1px solid var(--base-200);
		background: var(--base-100);
		font-size: 0.85rem;
		color: var(--base-content);
		transition: all 0.2s;
	}

	.checklist-add-input:focus {
		outline: none;
		border-color: var(--brand-500);
		box-shadow: 0 0 0 3px color-mix(in oklab, var(--brand-500) 15%, transparent);
	}

	.checklist-add-input::placeholder {
		color: color-mix(in oklab, var(--base-content) 45%, transparent);
	}

	.activity-list {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		margin-bottom: 1.25rem;
	}

	.activity-item {
		display: flex;
		gap: 0.75rem;
	}

	.activity-avatar {
		display: inline-flex;
		width: 2rem;
		height: 2rem;
		flex-shrink: 0;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		border-radius: 999px;
		background: color-mix(in oklab, var(--brand-500) 14%, white);
		color: var(--brand-700, #8b3d12);
		font-size: 0.75rem;
		font-weight: 800;
	}

	.activity-avatar img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.activity-body {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		flex: 1;
	}

	.activity-line {
		font-size: 0.85rem;
		line-height: 1.4;
	}

	.activity-comment {
		padding: 0.6rem 0.75rem;
		border-radius: 0.6rem;
		background: color-mix(in oklab, var(--base-200) 40%, white);
		font-size: 0.85rem;
		color: var(--base-content);
		line-height: 1.4;
	}

	.activity-time {
		font-size: 0.7rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}

	.comment-input-row {
		display: flex;
		gap: 0.75rem;
	}

	.comment-box {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.comment-textarea {
		width: 100%;
		padding: 0.6rem 0.75rem;
		border-radius: 0.6rem;
		border: 1px solid var(--base-200);
		background: var(--base-100);
		font-size: 0.85rem;
		color: var(--base-content);
		resize: vertical;
		min-height: 3.5rem;
		transition: all 0.2s;
	}

	.comment-textarea:focus {
		outline: none;
		border-color: var(--brand-500);
		box-shadow: 0 0 0 3px color-mix(in oklab, var(--brand-500) 15%, transparent);
	}

	.comment-actions {
		display: flex;
		justify-content: flex-end;
	}

	.detail-footer {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 1rem 1.5rem;
		border-top: 1px solid var(--base-200);
		background: color-mix(in oklab, var(--base-100) 95%, black);
		position: sticky;
		bottom: 0;
		z-index: 10;
		gap: 1rem;
		flex-wrap: wrap;
	}

	.footer-left,
	.footer-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
</style>
