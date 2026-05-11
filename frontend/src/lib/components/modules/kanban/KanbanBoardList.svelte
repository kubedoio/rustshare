<script lang="ts">
	import { Columns, MoreHorizontal } from 'lucide-svelte';
	import type { KanbanBoardSummary } from '$lib/api/types';

	interface Props {
		boards: KanbanBoardSummary[];
		onSelect: (boardId: string) => void;
	}

	let { boards, onSelect }: Props = $props();

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
</script>

<div class="flex flex-col gap-4">
	{#each boards as board}
		<button
			type="button"
			class="group flex w-full items-center gap-4 rounded-xl border border-base-300/40 bg-base-100 p-4 text-left transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
			onclick={() => onSelect(board.id)}
		>
			<div
				class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
			>
				<Columns size={18} />
			</div>
			<div class="flex min-w-0 flex-1 flex-col gap-1">
				<span class="text-sm font-semibold text-base-content">{board.title}</span>
				<span class="text-xs text-base-content/50">
					{board.column_count} column{board.column_count === 1 ? '' : 's'} · {board.card_count} card{board.card_count ===
					1
						? ''
						: 's'}
				</span>
				<span class="text-xs text-base-content/40">
					Updated {formatDate(board.updated_at)}
				</span>
			</div>
			<MoreHorizontal size={18} class="shrink-0 text-base-content/55" />
		</button>
	{/each}
</div>
