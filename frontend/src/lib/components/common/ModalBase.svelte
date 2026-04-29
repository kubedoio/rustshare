<script lang="ts">
	import { X } from 'lucide-svelte';

	interface Props {
		open: boolean;
		title: string;
		onClose: () => void;
		showCloseButton?: boolean;
		class?: string;
	}

	let { open, title, onClose, showCloseButton = true, class: className = '' }: Props = $props();

	let dialogRef: HTMLDialogElement | undefined = $state();
	let titleId = $derived(`modal-title-${Math.random().toString(36).slice(2)}`);
	let isProgrammaticClose = false;

	// Show/close dialog based on open prop
	$effect(() => {
		if (open) {
			if (dialogRef && !dialogRef.open) {
				dialogRef.showModal();
			}
		} else {
			if (dialogRef?.open) {
				isProgrammaticClose = true;
				dialogRef.close();
			}
		}
	});

	// Backdrop click to close
	$effect(() => {
		if (!dialogRef) return;
		const handler = (e: MouseEvent) => {
			if (e.target === dialogRef) {
				dialogRef?.close();
			}
		};
		dialogRef.addEventListener('click', handler);
		return () => dialogRef?.removeEventListener('click', handler);
	});

	// Handle close event (Escape or close())
	function handleClose() {
		if (isProgrammaticClose) {
			isProgrammaticClose = false;
			return;
		}
		onClose();
	}

	// Prevent body scroll when open
	$effect(() => {
		if (open) {
			const originalOverflow = document.body.style.overflow;
			document.body.style.overflow = 'hidden';
			return () => {
				document.body.style.overflow = originalOverflow;
			};
		}
	});

	// Focus management: focus first focusable element when opened
	$effect(() => {
		if (open && dialogRef) {
			requestAnimationFrame(() => {
				const focusable = dialogRef?.querySelector<HTMLElement>(
					'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
				);
				focusable?.focus();
			});
		}
	});
</script>

<dialog
	bind:this={dialogRef}
	class="fixed inset-0 z-50 m-0 h-full max-h-none w-full max-w-none bg-black/60 p-4 backdrop-blur-sm open:flex open:items-center open:justify-center"
	onclose={handleClose}
	aria-modal="true"
	aria-labelledby={titleId}
>
	<div
		class="relative w-full max-w-md overflow-hidden rounded-xl bg-base-100 shadow-2xl {className}"
	>
		{#if showCloseButton}
			<button
				type="button"
				class="absolute top-3 right-3 rounded-lg p-1 text-base-content/50 transition-colors hover:bg-base-200 hover:text-base-content"
				aria-label="Close"
				onclick={() => dialogRef?.close()}
			>
				<X size={20} />
			</button>
		{/if}

		<div class="border-b border-base-300/50 px-5 py-4">
			<h3 id={titleId} class="pr-8 text-lg font-semibold text-base-content">{title}</h3>
		</div>

		<div class="p-5">
			<slot />
		</div>
	</div>
</dialog>
