<script lang="ts">
	import { onMount } from 'svelte';
	import { Send, X, Paperclip, AlertTriangle, Save, Trash2 } from 'lucide-svelte';
	import type { SaveDraftRequest, SendOutboundMailRequest } from '$lib/api/mail';
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
		initialBcc = '',
		initialSubject = '',
		initialBody = '',
		initialAttachments = [],
		inReplyToMsgId = null,
		mode = 'new',
		draftId = null,
		sending = false,
		saving = false,
		discarding = false,
		hasSmtp = true,
		saveError = '',
		onClose,
		onSend,
		onSave,
		onDiscard
	}: {
		open: boolean;
		initialTo?: string;
		initialCc?: string;
		initialBcc?: string;
		initialSubject?: string;
		initialBody?: string;
		initialAttachments?: string[];
		inReplyToMsgId?: string | null;
		mode?: 'new' | 'reply' | 'reply-all' | 'forward' | 'draft-edit';
		draftId?: string | null;
		sending?: boolean;
		saving?: boolean;
		discarding?: boolean;
		hasSmtp?: boolean;
		saveError?: string;
		onClose: () => void;
		onSend: (message: SendOutboundMailRequest) => void;
		onSave: (message: SaveDraftRequest, draftId: string | null) => void | Promise<void>;
		onDiscard?: (draftId: string) => void;
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
	let saved = $state(false);
	let baseline = $state('');
	let idempotencyKey = $state('');

	onMount(async () => {
		try {
			files = await listAllFiles();
		} catch (err) {
			console.error('Failed to load files:', err);
		}
	});

	$effect(() => {
		if (open && !lastOpen) {
			idempotencyKey = crypto.randomUUID();
			draft = {
				to: initialTo,
				cc: initialCc,
				bcc: initialBcc,
				subject: initialSubject,
				body: initialBody,
				attachments: [...initialAttachments]
			};
			saved = false;
			baseline = JSON.stringify(draft);
		}
		lastOpen = open;
	});

	let changed = $derived(JSON.stringify(draft) !== baseline);

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

	function draftPayload(): SaveDraftRequest {
		return {
			to: splitAddresses(draft.to),
			cc: splitAddresses(draft.cc),
			bcc: splitAddresses(draft.bcc),
			subject: draft.subject.trim(),
			body: draft.body,
			attachments: draft.attachments,
			// Forward drafts must not persist the original as in_reply_to: the
			// send path would then emit In-Reply-To/References and thread the
			// forward as a reply in recipients' clients.
			in_reply_to_msg_id: mode === 'forward' ? null : inReplyToMsgId
		};
	}

	function handleSubmit() {
		onSend({ ...draftPayload(), idempotency_key: idempotencyKey });
	}

	async function handleSave() {
		try {
			await onSave(draftPayload(), draftId);
			saved = true;
			baseline = JSON.stringify(draft);
		} catch {
			// Save failed; the parent surfaces the error and the draft stays unsaved.
		}
	}

	function handleClose() {
		if (changed && !saved && !confirm('Close compose and lose unsaved changes?')) return;
		onClose();
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
					onclick={handleClose}
				>
					<X size={18} />
				</button>
			</div>

			{#if !hasSmtp}
				<div class="py-6 text-center">
					<div
						class="flex h-12 w-12 items-center justify-center rounded-xl bg-warning/10 text-warning mx-auto mb-4"
					>
						<AlertTriangle size={24} />
					</div>
					<h3 class="text-md font-bold text-base-content">Outgoing SMTP not configured</h3>
					<p class="text-sm text-base-content/60 mt-1 mb-6">
						Outgoing SMTP is not configured for this mail account. Configure SMTP in Settings to
						send mail.
					</p>
					<div class="flex justify-center gap-2">
						<button type="button" class="btn btn-sm btn-outline" onclick={onClose}>Close</button>
						<a href="/settings?tab=mail" class="btn btn-sm btn-primary">Open Mail settings</a>
					</div>
				</div>
			{:else}
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
						<input
							class="input input-bordered"
							type="text"
							placeholder="Cc"
							bind:value={draft.cc}
						/>
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

					{#if saveError}
						<p class="text-sm text-error">{saveError}</p>
					{:else if saved && !changed}
						<p class="text-sm text-success">Saved draft</p>
					{:else if changed && draftId}
						<p class="text-sm text-base-content/60">Unsaved changes</p>
					{/if}

					<div class="modal-action flex-wrap">
						{#if draftId && onDiscard}
							<button
								type="button"
								class="btn btn-error btn-outline gap-2"
								disabled={discarding || sending || saving}
								onclick={() => {
									if (confirm('Discard this draft permanently?')) onDiscard?.(draftId);
								}}
							>
								<Trash2 size={16} />
								{discarding ? 'Discarding...' : 'Discard'}
							</button>
						{/if}
						<button type="button" class="btn btn-outline" onclick={handleClose}>Cancel</button>
						<button
							type="button"
							class="btn btn-outline gap-2"
							disabled={saving ||
								sending ||
								discarding ||
								(!draft.subject.trim() && !draft.body.trim())}
							onclick={handleSave}
						>
							<Save size={16} />
							<span
								>{saving
									? 'Saving...'
									: mode === 'draft-edit'
										? 'Save changes'
										: 'Save draft'}</span
							>
						</button>
						<button type="submit" class="btn btn-primary gap-2" disabled={sending}>
							<Send size={16} />
							<span>{sending ? 'Sending...' : 'Send'}</span>
						</button>
					</div>
				</form>
			{/if}
		</div>
		<button class="modal-backdrop" type="button" aria-label="Close compose" onclick={handleClose}>
			Close
		</button>
	</div>
{/if}
