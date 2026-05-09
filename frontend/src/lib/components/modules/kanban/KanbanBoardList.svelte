<script lang="ts">
	import { Columns } from 'lucide-svelte';
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

<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
	{#each boards as board}
		<button
			type="button"
			class="group flex flex-col gap-3 rounded-xl border border-base-300/40 bg-base-100 p-5 text-left transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
			onclick={() => onSelect(board.id)}
		>
			<div class="flex items-start justify-between">
				<div
					class="flex h-10 w-10 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
				>
					<Columns size={18} />
				</div>
			</div>
			<div class="flex flex-col gap-1">
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
		</button>
	{/each}
</div>
