<script lang="ts">
	import ModalBase from './ModalBase.svelte';

	interface Props {
		open: boolean;
		title: string;
		message: string;
		defaultValue?: string;
		confirmLabel?: string;
		cancelLabel?: string;
		error?: string;
		onConfirm: (value: string) => void;
		onCancel: () => void;
	}

	let {
		open,
		title,
		message,
		defaultValue = '',
		confirmLabel = 'Create',
		cancelLabel = 'Cancel',
		error = '',
		onConfirm,
		onCancel
	}: Props = $props();

	let value = $state('');
	let inputRef: HTMLInputElement | undefined = $state();

	$effect(() => {
		if (open) {
			value = defaultValue;
			// Focus and select the input after the modal opens
			requestAnimationFrame(() => {
				inputRef?.focus();
				if (defaultValue) {
					inputRef?.select();
				}
			});
		}
	});

	function handleConfirm() {
		onConfirm(value);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			handleConfirm();
		}
	}
</script>

<ModalBase {open} {title} onClose={onCancel}>
	<div class="flex flex-col gap-4">
		<p class="text-sm text-base-content/80">{message}</p>
		{#if error}
			<p class="text-sm text-error">{error}</p>
		{/if}
		<input
			type="text"
			class="input-bordered input w-full"
			bind:this={inputRef}
			bind:value
			onkeydown={handleKeydown}
		/>
		<div class="flex justify-end gap-2">
			<button type="button" class="btn btn-ghost btn-sm" onclick={onCancel}>
				{cancelLabel}
			</button>
			<button type="button" class="btn btn-primary btn-sm" onclick={handleConfirm}>
				{confirmLabel}
			</button>
		</div>
	</div>
</ModalBase>
