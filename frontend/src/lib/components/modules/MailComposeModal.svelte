<script lang="ts">
	import { onMount } from 'svelte';
	import { Send, X, Paperclip } from 'lucide-svelte';
	import type { SendOutboundMailRequest } from '$lib/api/mail';
	import { listAllFiles } from '$lib/api/files';
	import type { File as WorkspaceFile } from '$lib/api/types';

	type Draft = {
		to: string;
		cc: string;
		bcc: string;
		subject: string;
		body: string;
		attachments: string[];
	};

	let {
		open,
		initialTo = '',
		initialCc = '',
		initialSubject = '',
		initialBody = '',
		initialAttachments = [],
		inReplyToMsgId = null,
		sending = false,
		onClose,
		onSend
	}: {
		open: boolean;
		initialTo?: string;
		initialCc?: string;
		initialSubject?: string;
		initialBody?: string;
		initialAttachments?: string[];
		inReplyToMsgId?: string | null;
		sending?: boolean;
		onClose: () => void;
		onSend: (message: SendOutboundMailRequest) => void;
	} = $props();

	let draft = $state<Draft>({
		to: '',
		cc: '',
		bcc: '',
		subject: '',
		body: '',
		attachments: []
	});

	let files = $state<WorkspaceFile[]>([]);
	let selectedFileIdToAdd = $state('');
	let lastOpen = $state(false);

	onMount(async () => {
		try {
			files = await listAllFiles();
		} catch (err) {
			console.error('Failed to load files:', err);
		}
	});

	$effect(() => {
		if (open && !lastOpen) {
			draft = {
				to: initialTo,
				cc: initialCc,
				bcc: '',
				subject: initialSubject,
				body: initialBody,
				attachments: [...initialAttachments]
			};
		}
		lastOpen = open;
	});

	let attachedFiles = $derived(
		draft.attachments
			.map((id) => files.find((f) => f.id === id))
			.filter((f): f is WorkspaceFile => !!f)
	);

	function addAttachment() {
		if (selectedFileIdToAdd && !draft.attachments.includes(selectedFileIdToAdd)) {
			draft.attachments.push(selectedFileIdToAdd);
			selectedFileIdToAdd = '';
		}
	}

	function removeAttachment(id: string) {
		draft.attachments = draft.attachments.filter((aid) => aid !== id);
	}

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
			body: draft.body,
			attachments: draft.attachments,
			in_reply_to_msg_id: inReplyToMsgId
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
					type="text"
					placeholder="To"
					bind:value={draft.to}
					required
				/>
				<div class="grid grid-cols-1 gap-3 md:grid-cols-2">
					<input class="input input-bordered" type="text" placeholder="Cc" bind:value={draft.cc} />
					<input
						class="input input-bordered"
						type="text"
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

				<!-- Attachments -->
				<div class="flex flex-col gap-2 rounded-lg border border-base-300 bg-base-200/30 p-3">
					<span class="flex items-center gap-2 text-xs font-semibold text-base-content/70">
						<Paperclip size={14} />
						Attachments
					</span>
					{#if attachedFiles.length > 0}
						<div class="flex flex-wrap gap-2 mb-1">
							{#each attachedFiles as file}
								<span class="badge badge-secondary gap-2 p-3">
									{file.name}
									<button
										type="button"
										class="btn btn-ghost btn-xs btn-square text-error-content hover:bg-error/20"
										onclick={() => removeAttachment(file.id)}
									>
										✕
									</button>
								</span>
							{/each}
						</div>
					{/if}
					<div class="flex gap-2">
						<select
							class="select select-sm select-bordered flex-1"
							bind:value={selectedFileIdToAdd}
						>
							<option value="">-- Link a file from workspace --</option>
							{#each files as file}
								{#if !draft.attachments.includes(file.id)}
									<option value={file.id}>{file.name}</option>
								{/if}
							{/each}
						</select>
						<button
							type="button"
							class="btn btn-sm btn-outline"
							onclick={addAttachment}
							disabled={!selectedFileIdToAdd}
						>
							Attach
						</button>
					</div>
				</div>

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
