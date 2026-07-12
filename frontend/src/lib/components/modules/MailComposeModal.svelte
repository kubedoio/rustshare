<script lang="ts">
	import { Send, X } from 'lucide-svelte';
	import type { SendMailMessageRequest } from '$lib/api/mail';

	type Draft = {
		to: string;
		cc: string;
		bcc: string;
		subject: string;
		body: string;
	};

	let {
		open,
		initialTo = '',
		initialCc = '',
		initialSubject = '',
		initialBody = '',
		sending = false,
		onClose,
		onSend
	}: {
		open: boolean;
		initialTo?: string;
		initialCc?: string;
		initialSubject?: string;
		initialBody?: string;
		sending?: boolean;
		onClose: () => void;
		onSend: (message: SendMailMessageRequest) => void;
	} = $props();

	let draft = $state<Draft>({
		to: '',
		cc: '',
		bcc: '',
		subject: '',
		body: ''
	});

	let lastOpen = $state(false);

	$effect(() => {
		if (open && !lastOpen) {
			draft = {
				to: initialTo,
				cc: initialCc,
				bcc: '',
				subject: initialSubject,
				body: initialBody
			};
		}
		lastOpen = open;
	});

	function splitAddresses(value: string): string[] {
		return value
			.split(',')
			.map((item) => item.trim())
			.filter(Boolean);
	}

	function handleSubmit() {
		onSend({
			to: splitAddresses(draft.to),
			cc: splitAddresses(draft.cc),
			bcc: splitAddresses(draft.bcc),
			subject: draft.subject.trim(),
			body: draft.body
		});
	}
</script>

{#if open}
	<div class="modal modal-open">
		<div class="modal-box max-w-3xl rounded-lg">
			<div class="mb-4 flex items-center justify-between gap-3">
				<h2 class="text-lg font-semibold">Compose</h2>
				<button
					type="button"
					class="btn btn-ghost btn-sm btn-square"
					aria-label="Close compose"
					onclick={onClose}
				>
					<X size={18} />
				</button>
			</div>

			<form
				class="flex flex-col gap-3"
				onsubmit={(event) => {
					event.preventDefault();
					handleSubmit();
				}}
			>
				<input
					class="input input-bordered"
					type="email"
					multiple
					placeholder="To"
					bind:value={draft.to}
					required
				/>
				<div class="grid grid-cols-1 gap-3 md:grid-cols-2">
					<input
						class="input input-bordered"
						type="email"
						multiple
						placeholder="Cc"
						bind:value={draft.cc}
					/>
					<input
						class="input input-bordered"
						type="email"
						multiple
						placeholder="Bcc"
						bind:value={draft.bcc}
					/>
				</div>
				<input
					class="input input-bordered"
					placeholder="Subject"
					bind:value={draft.subject}
					required
				/>
				<textarea
					class="textarea textarea-bordered min-h-72"
					placeholder="Message"
					bind:value={draft.body}
					required></textarea>

				<div class="modal-action">
					<button type="button" class="btn btn-outline" onclick={onClose}> Cancel </button>
					<button type="submit" class="btn btn-primary gap-2" disabled={sending}>
						<Send size={16} />
						<span>{sending ? 'Sending...' : 'Send'}</span>
					</button>
				</div>
			</form>
		</div>
		<button class="modal-backdrop" type="button" aria-label="Close compose" onclick={onClose}>
			Close
		</button>
	</div>
{/if}
