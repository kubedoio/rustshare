<script lang="ts">
	import {
		X,
		Plus,
		Check,
		Trash2,
		Paperclip,
		AlignLeft,
		CheckSquare,
		Activity,
		Folder as FolderIcon
	} from 'lucide-svelte';
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
		uploadingAttachment: boolean;
		onClose: () => void;
		onSave: () => void;
		onArchive: () => void;
		onDelete: () => void;
		onToggleLabel: (labelId: string) => void;
		onToggleAssignee: (userId: string) => void;
		onUploadAttachment: (e: Event) => void;
		onDeleteAttachment: (attachmentId: string) => void;
		onAddChecklist: (title: string) => void;
		onDeleteChecklist: (checklistId: string) => void;
		onAddChecklistItem: (checklistId: string, text: string) => void;
		onToggleItem: (checklistId: string, itemId: string, done: boolean) => void;
		onDeleteItem: (checklistId: string, itemId: string) => void;
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
		uploadingAttachment,
		onClose,
		onSave,
		onArchive,
		onDelete,
		onToggleLabel,
		onToggleAssignee,
		onUploadAttachment,
		onDeleteAttachment,
		onAddChecklist,
		onDeleteChecklist,
		onAddChecklistItem,
		onToggleItem,
		onDeleteItem,
		onCreateLabel
	}: Props = $props();

	let showLabelPicker = $state(false);
	let showAssigneePicker = $state(false);
	let showNewLabelForm = $state(false);
	let newLabelName = $state('');
	let newLabelColor = $state('blue');
	let newChecklistTitle = $state('');
	let newChecklistItemText = $state<Record<string, string>>({});

	function formatActivityDate(dateStr: string) {
		const date = new Date(dateStr);
		return new Intl.DateTimeFormat('en-US', {
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		}).format(date);
	}

	const EVENT_LABELS: Record<string, string> = {
		'card.created': 'Card created',
		'card.moved': 'Card moved',
		'card.title_changed': 'Title updated',
		'card.description_changed': 'Description updated',
		'card.label_added': 'Label added',
		'card.label_removed': 'Label removed',
		'card.assignee_added': 'Assignee added',
		'card.assignee_removed': 'Assignee removed',
		'card.checklist_added': 'Checklist added',
		'card.checklist_item_added': 'Checklist item added',
		'card.checklist_item_toggled': 'Checklist item completed',
		'card.attachment_added': 'Attachment added',
		'card.attachment_removed': 'Attachment removed',
		'card.due_date_changed': 'Due date updated',
		'card.archived': 'Card archived',
		'board.created': 'Board created',
		'board.renamed': 'Board renamed',
		'board.column_added': 'Column added'
	};

	function getEventLabel(eventType: string): string {
		return (
			EVENT_LABELS[eventType] ||
			eventType.replace(/card\./, '').replace(/\./g, ' ').replace(/_/g, ' ')
		);
	}

	function handleCreateLabel() {
		if (!newLabelName.trim()) return;
		onCreateLabel(newLabelName.trim(), newLabelColor);
		newLabelName = '';
		showNewLabelForm = false;
	}

	function handleAddChecklist() {
		if (!newChecklistTitle.trim()) return;
		onAddChecklist(newChecklistTitle.trim());
		newChecklistTitle = '';
	}

	function handleAddChecklistItem(checklistId: string) {
		const text = newChecklistItemText[checklistId];
		if (!text?.trim()) return;
		onAddChecklistItem(checklistId, text.trim());
		newChecklistItemText[checklistId] = '';
	}
</script>

<ModalBase {open} {onClose} {title}>
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
					<div class="detail-meta">
						in column <span class="font-bold">{card.status}</span>
					</div>
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
					<!-- Labels & Assignees -->
					<div class="detail-badges-row">
						<div class="detail-section">
							<h4 class="section-label">Labels</h4>
							<div class="flex flex-wrap items-center gap-1">
								{#each card.labels as label}
									<span class="card-label label-{label.color} group relative">
										{label.name}
										<button
											class="absolute -top-1.5 -right-1.5 hidden h-4 w-4 items-center justify-center rounded-full bg-base-content text-base-100 shadow-sm group-hover:flex"
											onclick={() => onToggleLabel(label.id)}
										>
											<X size={10} />
										</button>
									</span>
								{/each}
								<div class="relative">
									<button
										class="btn h-7 rounded-lg border border-dashed border-base-300 px-2 btn-ghost btn-xs hover:border-brand-500"
										onclick={() => {
											showLabelPicker = !showLabelPicker;
											showAssigneePicker = false;
										}}
									>
										<Plus size={12} class="mr-1" />
										<span>Add</span>
									</button>
									{#if showLabelPicker}
										<div
											class="absolute top-full left-0 z-50 mt-1 min-w-[200px] rounded-xl border border-base-300 bg-base-100 p-3 shadow-xl"
										>
											<div class="mb-2 flex items-center justify-between">
												<div class="text-[10px] font-bold text-base-content/40 uppercase">
													Select Label
												</div>
												<button
													class="btn h-6 px-1 text-brand-500 btn-ghost btn-xs hover:bg-brand-50"
													onclick={() => (showNewLabelForm = !showNewLabelForm)}
												>
													{showNewLabelForm ? 'Cancel' : 'New'}
												</button>
											</div>

											{#if showNewLabelForm}
												<div class="mb-3 flex flex-col gap-2 rounded-lg bg-base-200/50 p-2">
													<input
														type="text"
														placeholder="Label name..."
														class="input input-xs h-8 bg-base-100"
														bind:value={newLabelName}
														onkeydown={(e) => e.key === 'Enter' && handleCreateLabel()}
													/>
													<div class="flex flex-wrap gap-1">
														{#each ['green', 'yellow', 'orange', 'red', 'purple', 'blue', 'gray'] as color}
															<button
																aria-label={color}
																class="h-5 w-5 rounded-full label-{color} border-2 {newLabelColor ===
																color
																	? 'border-base-content'
																	: 'border-transparent'}"
																onclick={() => (newLabelColor = color)}
															></button>
														{/each}
													</div>
													<button
														class="btn h-8 w-full btn-xs btn-primary"
														disabled={!newLabelName.trim()}
														onclick={handleCreateLabel}
													>
														Create Label
													</button>
												</div>
											{/if}

											<div class="flex max-h-48 flex-col gap-1 overflow-y-auto">
												{#each board.labels as label}
													<button
														class="flex items-center justify-between rounded-lg px-2 py-1.5 text-left text-xs hover:bg-base-200"
														onclick={() => onToggleLabel(label.id)}
													>
														<span class="card-label label-{label.color} !m-0">{label.name}</span>
														{#if card.labels.some((l) => l.id === label.id)}
															<Check size={12} class="text-brand-500" />
														{/if}
													</button>
												{/each}
											</div>
										</div>
									{/if}
								</div>
							</div>
						</div>

						<div class="detail-section">
							<h4 class="section-label">Assignees</h4>
							<div class="flex flex-wrap items-center gap-1">
								{#each card.assignees as assignee}
									<div class="assignee-avatar group relative" title={assignee.display_name}>
										{#if assignee.avatar_url}
											<img src={assignee.avatar_url} alt={assignee.display_name} />
										{:else}
											<span>{assignee.initials}</span>
										{/if}
										<button
											class="absolute -top-1 -right-1 z-10 hidden h-4 w-4 items-center justify-center rounded-full bg-base-content text-base-100 shadow-sm group-hover:flex"
											onclick={() => onToggleAssignee(assignee.id)}
										>
											<X size={10} />
										</button>
									</div>
								{/each}
								<div class="relative">
									<button
										class="btn h-8 w-8 rounded-full border border-dashed border-base-300 p-0 btn-ghost btn-xs hover:border-brand-500"
										onclick={() => {
											showAssigneePicker = !showAssigneePicker;
											showLabelPicker = false;
										}}
									>
										<Plus size={14} />
									</button>
									{#if showAssigneePicker}
										<div
											class="absolute top-full left-0 z-50 mt-1 min-w-[200px] rounded-xl border border-base-300 bg-base-100 p-2 shadow-xl"
										>
											<div class="mb-1 px-2 text-[10px] font-bold text-base-content/40 uppercase">
												Assign Member
											</div>
											<div class="flex max-h-48 flex-col gap-1 overflow-y-auto">
												{#each assignableUsers as user}
													<button
														class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs hover:bg-base-200"
														onclick={() => onToggleAssignee(user.id)}
													>
														<div class="assignee-avatar !h-6 !w-6 !text-[10px]">
															{#if user.avatar_url}
																<img src={user.avatar_url} alt={user.display_name} />
															{:else}
																<span>{user.initials}</span>
															{/if}
														</div>
														<span class="flex-1 truncate">{user.display_name}</span>
														{#if card.assignees.some((a) => a.id === user.id)}
															<Check size={12} class="text-brand-500" />
														{/if}
													</button>
												{/each}
											</div>
										</div>
									{/if}
								</div>
							</div>
						</div>
					</div>

					<!-- Description -->
					<div class="detail-section">
						<div class="mb-2 flex items-center gap-2">
							<AlignLeft size={18} class="text-base-content/60" />
							<h4 class="section-label !mb-0">Description</h4>
						</div>
						<div class="description-editor">
							<RichMarkdownEditor
								bind:content={card.content}
								editable={true}
								on:change={() => {
									// saveStatus is managed by parent via saveStatus prop
								}}
								hasAttachmentHandler={true}
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
									Save Changes
								</button>
							</div>
						</div>
					</div>

					<!-- Attachments -->
					<div class="detail-section">
						<div class="mb-4 flex items-center justify-between">
							<div class="flex items-center gap-2">
								<Paperclip size={18} class="text-base-content/60" />
								<h4 class="section-label !mb-0">Attachments</h4>
							</div>
							<label class="btn gap-1 btn-ghost btn-xs">
								<Plus size={14} />
								Add
								<input type="file" class="hidden" onchange={onUploadAttachment} />
							</label>
						</div>

						{#if card.attachments.length > 0}
							<div class="grid grid-cols-1 gap-2">
								{#each card.attachments as attachment}
									<div class="attachment-item group">
										<div class="attachment-icon">
											<FolderIcon size={16} />
										</div>
										<div class="attachment-info flex-1">
											<div class="attachment-name">{attachment.name}</div>
											<div class="attachment-meta">
												{Math.round(attachment.size / 1024)} KB • {formatActivityDate(
													attachment.created_at
												)}
											</div>
										</div>
										<button
											class="btn text-error opacity-0 btn-ghost btn-xs group-hover:opacity-100"
											onclick={() => onDeleteAttachment(attachment.id)}
										>
											<Trash2 size={14} />
										</button>
									</div>
								{/each}
							</div>
						{:else}
							<div class="px-2 text-xs text-base-content/40 italic">No attachments yet.</div>
						{/if}

						{#if uploadingAttachment}
							<div class="mt-2 flex items-center gap-2 text-xs text-base-content/60">
								<span class="loading loading-xs loading-spinner"></span>
								Uploading...
							</div>
						{/if}
					</div>

					<!-- Checklists -->
					<div class="detail-section">
						<div class="mb-4 flex items-center justify-between">
							<div class="flex items-center gap-2">
								<CheckSquare size={18} class="text-base-content/60" />
								<h4 class="section-label !mb-0">Checklists</h4>
							</div>
							<div class="flex items-center gap-2">
								<input
									type="text"
									placeholder="New checklist..."
									class="input-bordered input input-xs w-32"
									bind:value={newChecklistTitle}
									onkeydown={(e) => e.key === 'Enter' && handleAddChecklist()}
								/>
								<button class="btn btn-xs btn-primary" onclick={handleAddChecklist}>Add</button>
							</div>
						</div>

						{#each card.checklists as checklist}
							<div class="mb-6 rounded-xl border border-base-200/50 bg-base-200/20 p-4 last:mb-0">
								<div class="mb-2 flex items-center justify-between">
									<h5 class="flex items-center gap-2 text-sm font-bold">
										{checklist.title}
										<span
											class="rounded-full bg-base-200 px-1.5 py-0.5 text-[10px] font-medium text-base-content/60"
										>
											{checklist.items.filter((i) => i.done).length}/{checklist.items.length}
										</span>
									</h5>
									<button
										class="btn text-error/40 btn-ghost btn-xs hover:text-error"
										onclick={() => onDeleteChecklist(checklist.id)}
									>
										<Trash2 size={12} />
									</button>
								</div>

								<div class="mb-3 h-1.5 w-full overflow-hidden rounded-full bg-base-200">
									<div
										class="h-full bg-success transition-all duration-300"
										style="width: {(checklist.items.filter((i) => i.done).length /
											(checklist.items.length || 1)) *
											100}%"
									></div>
								</div>

								<div class="mb-3 flex flex-col gap-1">
									{#each checklist.items as item}
										<div
											class="group flex items-center gap-2 rounded-lg p-1 transition-colors hover:bg-base-200/50"
										>
											<input
												type="checkbox"
												checked={item.done}
												class="checkbox checkbox-xs checkbox-primary"
												onchange={(e) =>
													onToggleItem(
														checklist.id,
														item.id,
														(e.target as HTMLInputElement).checked
													)}
											/>
											<span
												class="flex-1 text-sm"
												class:line-through={item.done}
												class:opacity-50={item.done}
											>
												{item.text}
											</span>
											<button
												class="btn text-base-content/20 opacity-0 btn-ghost btn-xs group-hover:opacity-100 hover:text-error"
												onclick={() => onDeleteItem(checklist.id, item.id)}
											>
												<X size={12} />
											</button>
										</div>
									{/each}
								</div>

								<div class="flex items-center gap-2">
									<input
										type="text"
										placeholder="Add an item..."
										class="input-bordered input input-xs flex-1"
										bind:value={newChecklistItemText[checklist.id]}
										onkeydown={(e) => e.key === 'Enter' && handleAddChecklistItem(checklist.id)}
									/>
									<button
										class="btn btn-ghost btn-xs"
										onclick={() => handleAddChecklistItem(checklist.id)}
									>
										Add
									</button>
								</div>
							</div>
						{/each}

						{#if card.checklists.length === 0}
							<div class="px-2 text-xs text-base-content/40 italic">No checklists yet.</div>
						{/if}
					</div>

					<!-- Activity -->
					<div class="detail-section">
						<div class="mb-4 flex items-center gap-2">
							<Activity size={18} class="text-base-content/60" />
							<h4 class="section-label !mb-0">Activity</h4>
						</div>
						<div class="activity-feed">
							{#each card.activity as event}
								<div class="activity-item">
									<div class="activity-avatar">
										{event.actor.charAt(0).toUpperCase()}
									</div>
									<div class="activity-body">
										<div class="activity-header">
											<span class="font-bold">{event.actor}</span>
											<span class="ml-1 text-base-content/40"
												>{formatActivityDate(event.timestamp)}</span
											>
										</div>
										<div class="activity-text">
											{getEventLabel(event.event_type)}
										</div>
									</div>
								</div>
							{/each}
						</div>
					</div>

					<!-- Card Actions -->
					<div class="detail-section flex gap-2">
						<button class="btn btn-sm btn-outline" onclick={onArchive}>Archive</button>
						<button class="btn btn-sm btn-error btn-outline" onclick={onDelete}>Delete</button>
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
		overflow-y: auto;
		background: var(--rs-surface-primary, white);
		display: flex;
		flex-direction: column;
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
	}

	.detail-section {
		margin-bottom: 2rem;
	}

	.section-label {
		font-size: 0.85rem;
		font-weight: 700;
		color: color-mix(in oklab, var(--base-content) 70%, transparent);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin-bottom: 0.75rem;
	}

	.detail-badges-row {
		display: flex;
		flex-wrap: wrap;
		gap: 2rem;
		margin-bottom: 1rem;
	}

	.description-editor {
		background: color-mix(in oklab, var(--rs-surface-muted) 30%, white);
		border-radius: 0.75rem;
		border: 1px solid var(--base-200);
		padding: 0.5rem;
	}

	.attachment-item {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.75rem;
		background: var(--base-100);
		border: 1px solid var(--base-200);
		border-radius: 0.75rem;
		transition: all 0.2s;
	}

	.attachment-item:hover {
		background: white;
		border-color: var(--brand-500);
		cursor: pointer;
	}

	.attachment-icon {
		width: 2.5rem;
		height: 2.5rem;
		background: var(--base-200);
		border-radius: 0.5rem;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--base-content);
	}

	.attachment-name {
		font-weight: 700;
		font-size: 0.9rem;
	}

	.attachment-meta {
		font-size: 0.75rem;
		color: var(--base-content);
		opacity: 0.6;
	}

	.activity-feed {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}

	.activity-item {
		display: flex;
		gap: 1rem;
	}

	.activity-avatar {
		width: 2rem;
		height: 2rem;
		border-radius: 999px;
		background: var(--brand-500);
		color: white;
		display: flex;
		align-items: center;
		justify-content: center;
		font-weight: 800;
		font-size: 0.8rem;
		flex-shrink: 0;
	}

	.activity-header {
		font-size: 0.85rem;
	}

	.activity-text {
		font-size: 0.9rem;
		color: var(--base-content);
		margin-top: 0.15rem;
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

	@media (max-width: 767px) {
		.detail-badges-row {
			gap: 1rem;
		}
	}
</style>
