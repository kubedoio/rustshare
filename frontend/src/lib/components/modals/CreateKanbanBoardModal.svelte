<script lang="ts">
	import { onMount } from 'svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import { createKanbanBoard } from '$lib/api/kanban';
	import { createFromTemplate } from '$lib/api/modules';
	import { createMutation, useQueryClient } from '$lib/query-compat';

	interface Props {
		open: boolean;
		onClose: () => void;
		onSuccess: (boardId: string) => void;
		defaultTemplate: string | null;
	}

	let { open, onClose, onSuccess, defaultTemplate }: Props = $props();

	let boardName = $state('');
	let isSubmitting = $state(false);
	let error = $state('');

	let inputElement: HTMLInputElement;

	onMount(() => {
		if (open && inputElement) {
			inputElement.focus();
		}
	});

	$effect(() => {
		if (open && inputElement) {
			inputElement.focus();
		}
	});

	const queryClient = useQueryClient();

	async function handleSubmit() {
		if (!boardName.trim()) return;
		isSubmitting = true;
		error = '';

		try {
			let boardId: string;
			if (defaultTemplate) {
				const result = await createFromTemplate({
					template_key: defaultTemplate,
					name: boardName.trim(),
					parent_folder_id: null
				});
				boardId = result.object_id;
			} else {
				const result = await createKanbanBoard(boardName.trim());
				boardId = result.id;
			}

			queryClient.invalidateQueries({ queryKey: ['kanban-boards'] });
			onSuccess(boardId);
			boardName = '';
			onClose();
		} catch (err: any) {
			error = err.message || 'Failed to create board';
		} finally {
			isSubmitting = false;
		}
	}
</script>

<ModalBase {open} {onClose} title="Create New Kanban Board">
	<form
		onsubmit={(e) => {
			e.preventDefault();
			handleSubmit();
		}}
		class="flex flex-col gap-4"
	>
		<div>
			<label
				for="board-name"
				class="label-text mb-1 block text-xs font-semibold text-base-content/70">Board Name</label
			>
			<input
				id="board-name"
				bind:this={inputElement}
				type="text"
				placeholder="e.g. Project Roadmap"
				class="input-bordered input w-full"
				bind:value={boardName}
				disabled={isSubmitting}
			/>
		</div>

		{#if error}
			<div class="rounded-lg bg-red-50 px-3 py-2 text-xs text-red-600">
				{error}
			</div>
		{/if}

		<div class="flex justify-end gap-2 pt-2">
			<button type="button" class="btn btn-ghost btn-sm" onclick={onClose} disabled={isSubmitting}
				>Cancel</button
			>
			<button
				type="submit"
				class="btn btn-sm btn-primary"
				disabled={!boardName.trim() || isSubmitting}
			>
				{#if isSubmitting}
					<span class="loading loading-xs loading-spinner"></span>
				{/if}
				Create Board
			</button>
		</div>
	</form>
</ModalBase>
