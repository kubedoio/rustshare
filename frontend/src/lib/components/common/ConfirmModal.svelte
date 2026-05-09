<script lang="ts">
	import ModalBase from './ModalBase.svelte';

	interface Props {
		open: boolean;
		title: string;
		message: string;
		confirmLabel?: string;
		cancelLabel?: string;
		danger?: boolean;
		onConfirm: () => void;
		onCancel: () => void;
	}

	let {
		open,
		title,
		message,
		confirmLabel = 'Confirm',
		cancelLabel = 'Cancel',
		danger = false,
		onConfirm,
		onCancel
	}: Props = $props();
</script>

<ModalBase {open} {title} onClose={onCancel}>
	<div class="flex flex-col gap-4">
		<p class="text-sm text-base-content/80">{message}</p>
		<div class="flex justify-end gap-2">
			<button type="button" class="btn btn-ghost btn-sm" onclick={onCancel}>
				{cancelLabel}
			</button>
			<button
				type="button"
				class="btn btn-sm {danger ? 'btn-error' : 'btn-primary'}"
				onclick={onConfirm}
			>
				{confirmLabel}
			</button>
		</div>
	</div>
</ModalBase>
