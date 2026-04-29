<script lang="ts">
	interface Props {
		message: string;
		type?: 'success' | 'error' | 'info';
		duration?: number;
		onClose?: () => void;
	}

	let { message, type = 'info', duration = 3000, onClose = () => {} }: Props = $props();

	let visible = $state(true);

	$effect(() => {
		if (duration > 0) {
			const timer = setTimeout(() => {
				visible = false;
				setTimeout(onClose, 300); // Wait for fade out
			}, duration);

			return () => clearTimeout(timer);
		}
	});

	const alertClass = {
		success: 'alert-success',
		error: 'alert-error',
		info: 'alert-info'
	}[type];
</script>

{#if visible}
	<div class="toast toast-end toast-top z-[100] mt-16">
		<div class="alert {alertClass} shadow-lg">
			<span>{message}</span>
		</div>
	</div>
{/if}
