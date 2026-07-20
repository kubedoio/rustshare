<script lang="ts">
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import { Check, Loader2 } from 'lucide-svelte';

	interface Props {
		open: boolean;
		count?: number;
		alreadySaved?: boolean;
		importedMessageId?: string | null;
		isLoading?: boolean;
		onClose: () => void;
		onConfirm: () => void;
	}

	let {
		open,
		count = 1,
		alreadySaved = false,
		importedMessageId = null,
		isLoading = false,
		onClose,
		onConfirm
	}: Props = $props();
</script>

<ModalBase {open} title="Save to RustShare" {onClose}>
	<div class="flex flex-col gap-4">
		<p class="text-sm text-base-content/80">
			This creates a durable RustShare copy of the original .eml, metadata, and attachments. The
			copy is independent of the mailbox and becomes searchable.
		</p>

		{#if alreadySaved}
			<div class="flex items-start gap-3 rounded-lg bg-brand-500/10 p-3 text-sm text-brand-700">
				<Check size={18} class="mt-0.5 shrink-0" />
				<div>
					<p class="font-medium">Already saved to RustShare</p>
					{#if importedMessageId}
						<a
							href="/modules/mail/messages/{importedMessageId}"
							class="link link-primary text-sm"
							onclick={onClose}
						>
							Open saved copy
						</a>
					{/if}
				</div>
			</div>
		{:else}
			<p class="text-sm text-base-content/60">
				{count === 1 ? 'One message will be imported.' : `${count} messages will be imported.`}
			</p>
		{/if}

		<div class="flex justify-end gap-2">
			<button type="button" class="btn btn-ghost btn-sm" onclick={onClose}>Cancel</button>
			<button
				type="button"
				class="btn btn-primary btn-sm"
				disabled={isLoading || alreadySaved}
				onclick={onConfirm}
			>
				{#if isLoading}
					<Loader2 size={14} class="animate-spin" />
				{/if}
				<span>Save to RustShare</span>
			</button>
		</div>
	</div>
</ModalBase>
