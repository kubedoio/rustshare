<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { getFolderContents } from '$lib/api/folders';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import type { File, Folder as FolderType } from '$lib/api/types';
	import { Folder, Plus, ArrowRight, GripVertical, FileText } from 'lucide-svelte';

	export let moduleConfig: {
		module_key: string;
		display_name: string;
		description: string;
		icon: string;
		root_path: string;
		default_template: string | null;
		ui_config?: {
			modulePage?: {
				emptyStateTitle?: string;
				emptyStateDescription?: string;
				emptyStateAction?: string;
			};
		};
	};

	$: emptyTitle = moduleConfig.ui_config?.modulePage?.emptyStateTitle ?? 'No boards yet';
	$: emptyDescription =
		moduleConfig.ui_config?.modulePage?.emptyStateDescription ??
		'Create your first kanban board to get started.';
	$: emptyAction = moduleConfig.ui_config?.modulePage?.emptyStateAction ?? 'New Board';

	// Fetch module root folder contents
	$: rootFolderQuery = createQuery({
		queryKey: ['kanban-root', moduleConfig.module_key],
		queryFn: async () => {
			const res = await fetch('/api/v1/folders/root/contents');
			if (!res.ok) throw new Error('Failed to fetch root contents');
			const data = await res.json();
			const rootName = moduleConfig.root_path.replace(/^\//, '');
			const folder = data.folders?.find((f: { name: string }) => f.name === rootName);
			if (!folder) return { folders: [], files: [], current_folder: null };
			const contents = await getFolderContents(folder.id);
			const boardCandidates = await Promise.all(
				(contents.folders ?? []).map(async (candidate) => {
					const boardContents = await getFolderContents(candidate.id);
					const visibleColumns = boardContents.folders.filter((column) =>
						isKanbanColumn(column.name)
					);
					const isValid =
						boardContents.files.some((file) => file.name === KANBAN_METADATA_FILE) ||
						visibleColumns.length >= 3;

					return {
						...candidate,
						columnCount: visibleColumns.length,
						isValid
					};
				})
			);

			return {
				folders: boardCandidates
					.filter((candidate) => candidate.isValid)
					.map(({ isValid: _ignored, ...candidate }) => candidate),
				files: contents.files,
				current_folder: folder,
				ignoredFolderCount: boardCandidates.filter((candidate) => !candidate.isValid).length
			};
		},
		enabled: true
	});

	$: contents = $rootFolderQuery.data;
	$: boards = [...((contents?.folders as BoardFolder[]) ?? [])].sort((a, b) =>
		a.name.localeCompare(b.name, undefined, { numeric: true })
	);
	$: cards = contents?.files ?? [];
	$: ignoredFolderCount = contents?.ignoredFolderCount ?? 0;
	let selectedBoardId = '';
	$: if (!selectedBoardId && boards.length > 0) {
		selectedBoardId = boards[0].id;
	}
	$: if (
		selectedBoardId &&
		boards.length > 0 &&
		!boards.some((board) => board.id === selectedBoardId)
	) {
		selectedBoardId = boards[0].id;
	}
	$: selectedBoard = boards.find((board) => board.id === selectedBoardId) ?? null;

	type BoardCard = {
		id: string;
		name: string;
		itemType: 'file' | 'folder';
		updatedAt: string;
	};

	type BoardColumn = {
		id: string;
		name: string;
		cards: BoardCard[];
	};

	type BoardFolder = FolderType & {
		columnCount: number;
	};

	const STANDARD_KANBAN_COLUMNS = new Set(['backlog', 'ready', 'in progress', 'review', 'done']);
	const KANBAN_METADATA_FILE = '.rustshare-module.json';

	$: boardQuery = createQuery({
		queryKey: ['kanban-board', moduleConfig.module_key, selectedBoardId],
		queryFn: async () => {
			if (!selectedBoardId) {
				return { columns: [] as BoardColumn[] };
			}

			const boardContents = await getFolderContents(selectedBoardId);
			const columns = await Promise.all(
				[...boardContents.folders]
					.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }))
					.map(async (column) => {
						const columnContents = await getFolderContents(column.id);
						return {
							id: column.id,
							name: formatColumnName(column.name),
							cards: [
								...columnContents.folders
									.filter((item) => !isHiddenItem(item.name))
									.map(
										(item): BoardCard => ({
											id: item.id,
											name: item.name,
											itemType: 'folder',
											updatedAt: item.updated_at
										})
									),
								...columnContents.files
									.filter((item) => !isHiddenItem(item.name))
									.map(
										(item): BoardCard => ({
											id: item.id,
											name: item.name.replace(/\.[^.]+$/, ''),
											itemType: 'file',
											updatedAt: item.modified_at
										})
									)
							].sort((a, b) => Date.parse(b.updatedAt) - Date.parse(a.updatedAt))
						} satisfies BoardColumn;
					})
			);

			return { columns };
		},
		enabled: Boolean(selectedBoardId)
	});

	$: boardColumns = $boardQuery.data?.columns ?? [];

	async function handleCreateBoard() {
		if (!moduleConfig.default_template) return;
		const name = window.prompt('Enter a name for the new board:');
		if (!name) return;
		try {
			await createFromTemplate({
				template_key: moduleConfig.default_template,
				name,
				parent_folder_id: null
			});
			$rootFolderQuery.refetch();
			selectedBoardId = '';
		} catch (err) {
			console.error('Failed to create board:', err);
		}
	}

	function navigateToBoard(folderId: string) {
		goto(`/files?folder=${folderId}`);
	}

	function openCard(card: BoardCard, boardId: string) {
		if (card.itemType === 'file') {
			goto(`/files?preview=${card.id}`);
			return;
		}

		goto(`/files?folder=${card.id}&fromBoard=${boardId}`);
	}

	function formatColumnName(value: string): string {
		return value.replace(/^\d+-/, '').replace(/-/g, ' ');
	}

	function isHiddenItem(value: string): boolean {
		return value.startsWith('.');
	}

	function isKanbanColumn(value: string): boolean {
		return STANDARD_KANBAN_COLUMNS.has(formatColumnName(value).trim().toLowerCase());
	}
</script>

<div class="flex flex-col gap-6">
	{#if boards.length === 0 && cards.length === 0}
		<EmptyState
			icon={Folder}
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateBoard}
		/>
	{:else}
		<div class="flex items-center justify-between gap-4">
			<div class="flex flex-col gap-2">
				<h2 class="text-sm font-semibold tracking-wider text-base-content uppercase">Boards</h2>
				{#if ignoredFolderCount > 0}
					<p class="text-xs text-base-content/50">
						Ignoring {ignoredFolderCount} folder{ignoredFolderCount === 1 ? '' : 's'} that do not match
						the Kanban board structure.
					</p>
				{/if}
				{#if boards.length > 0}
					<div class="flex flex-wrap gap-2">
						{#each boards as board}
							<button
								type="button"
								class={`rounded-full border px-3 py-1.5 text-xs font-semibold transition-colors ${
									board.id === selectedBoardId
										? 'border-brand-500 bg-brand-500 text-white'
										: 'border-base-300/60 bg-base-100 text-base-content/70 hover:border-brand-500/40'
								}`}
								onclick={() => {
									selectedBoardId = board.id;
								}}
							>
								{board.name}
							</button>
						{/each}
					</div>
				{/if}
			</div>
			<button class="btn btn-sm btn-primary" onclick={handleCreateBoard}>
				<Plus size={14} />
				<span>New Board</span>
			</button>
		</div>

		{#if selectedBoard}
			<div class="flex items-center justify-between">
				<div>
					<h3 class="text-lg font-semibold text-base-content">{selectedBoard.name}</h3>
					<p class="text-sm text-base-content/55">
						File-backed board preview with {selectedBoard.columnCount} active column{selectedBoard.columnCount ===
						1
							? ''
							: 's'}.
					</p>
				</div>
				<button
					type="button"
					class="inline-flex items-center gap-2 rounded-full border border-base-300/70 bg-base-100 px-4 py-2 text-sm font-semibold text-base-content transition-colors hover:border-brand-500/40"
					onclick={() => navigateToBoard(selectedBoard.id)}
				>
					<span>Open Board Folder</span>
					<ArrowRight size={14} />
				</button>
			</div>

			{#if $boardQuery.isLoading}
				<div
					class="flex h-48 items-center justify-center rounded-3xl border border-base-300/40 bg-base-100"
				>
					<div class="loading loading-md loading-spinner text-brand-500"></div>
				</div>
			{:else if boardColumns.length === 0}
				<div
					class="rounded-3xl border border-base-300/40 bg-base-100 p-6 text-sm text-base-content/55"
				>
					This board does not have any visible columns yet.
				</div>
			{:else}
				<div class="kanban-board-surface">
					{#each boardColumns as column}
						<section class="kanban-column">
							<header class="kanban-column-header">
								<h4>{column.name}</h4>
								<span>{column.cards.length}</span>
							</header>

							<div class="kanban-card-list">
								{#if column.cards.length === 0}
									<div class="kanban-empty-column">No cards</div>
								{:else}
									{#each column.cards as card}
										<button
											type="button"
											class="kanban-card"
											onclick={() => openCard(card, selectedBoard.id)}
										>
											<div class="kanban-card-title-row">
												{#if card.itemType === 'file'}
													<FileText size={14} class="text-brand-500" />
												{:else}
													<Folder size={14} class="text-brand-500" />
												{/if}
												<strong>{card.name}</strong>
											</div>
											<p>Updated {new Date(card.updatedAt).toLocaleDateString()}</p>
										</button>
									{/each}
								{/if}
							</div>
						</section>
					{/each}
				</div>
			{/if}
		{:else}
			<p class="text-sm text-base-content/50">
				No boards yet. Create your first board to get started.
			</p>
		{/if}
	{/if}
</div>

<style>
	.kanban-board-surface {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(15rem, 1fr);
		gap: 1rem;
		overflow-x: auto;
		padding-bottom: 0.75rem;
	}

	.kanban-column {
		display: flex;
		min-height: 26rem;
		flex-direction: column;
		gap: 0.9rem;
		border-radius: 1.5rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 52%, transparent);
		background: color-mix(in oklab, var(--rs-surface-muted) 38%, white);
		padding: 1rem;
	}

	.kanban-column-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
	}

	.kanban-column-header h4 {
		margin: 0;
		font-size: 0.95rem;
		font-weight: 800;
		color: var(--base-content);
	}

	.kanban-column-header span {
		display: inline-flex;
		min-width: 1.75rem;
		justify-content: center;
		border-radius: 999px;
		background: rgba(255, 255, 255, 0.8);
		padding: 0.18rem 0.45rem;
		font-size: 0.72rem;
		font-weight: 700;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
	}

	.kanban-card-list {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 0.8rem;
	}

	.kanban-card {
		display: flex;
		flex-direction: column;
		gap: 0.45rem;
		border-radius: 1rem;
		border: 1px solid rgba(133, 95, 44, 0.08);
		background: rgba(255, 255, 255, 0.85);
		padding: 0.95rem;
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
		align-items: center;
		gap: 0.55rem;
	}

	.kanban-card strong {
		font-size: 0.92rem;
		font-weight: 700;
		color: var(--base-content);
	}

	.kanban-card p,
	.kanban-empty-column {
		margin: 0;
		font-size: 0.78rem;
		color: color-mix(in oklab, var(--base-content) 58%, transparent);
	}

	.kanban-empty-column {
		border: 1px dashed color-mix(in oklab, var(--base-300) 60%, transparent);
		border-radius: 1rem;
		background: rgba(255, 255, 255, 0.45);
		padding: 1rem;
	}

	@media (max-width: 767px) {
		.kanban-board-surface {
			grid-auto-columns: minmax(14rem, 16rem);
		}
	}
</style>
