<script lang="ts">
	import { onMount } from 'svelte';
	import { Send, X, Paperclip, AlertTriangle, Save, Trash2 } from 'lucide-svelte';
	import type { SaveDraftRequest, SendOutboundMailRequest } from '$lib/api/mail';
	import { listAllFiles } from '$lib/api/files';
	import type { File as WorkspaceFile } from '$lib/api/types';
	import MailBodyEditor from '$lib/components/modules/mail/MailBodyEditor.svelte';
	import { markdownToHtml } from '$lib/editor/adapter/markdown';
	import { sanitizeHtml } from '$lib/editor/adapter/security';

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
	let showCc = $state(false);
	let showBcc = $state(false);

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
			showCc = initialCc.trim().length > 0;
			showBcc = initialBcc.trim().length > 0;
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

	/** Render the Markdown body to sanitized HTML for the multipart/alternative part. */
	function bodyHtml(): string | null {
		if (!draft.body.trim()) return null;
		const rendered = markdownToHtml(draft.body);
		if (!rendered.success || !rendered.html.trim()) return null;
		return sanitizeHtml(rendered.html);
	}

	function draftPayload(): SaveDraftRequest {
		return {
			to: splitAddresses(draft.to),
			cc: splitAddresses(draft.cc),
			bcc: splitAddresses(draft.bcc),
			subject: draft.subject.trim(),
			body: draft.body,
			body_html: bodyHtml(),
			attachments: draft.attachments,
			// Forward drafts must not persist the original as in_reply_to: the
			// send path would then emit In-Reply-To/References and thread the
			// forward as a reply in recipients' clients.
			in_reply_to_msg_id: mode === 'forward' ? null : inReplyToMsgId
		};
	}

	function handleSubmit() {
		if (!draft.body.trim()) return;
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
		<div
			class="modal-box max-w-2xl rounded-lg border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] p-0"
		>
			<div class="flex items-center justify-between border-b border-[var(--rs-border)] px-4 py-2.5">
				<h2 class="text-sm font-semibold text-base-content">Compose</h2>
				<button
					type="button"
					class="btn btn-ghost btn-sm btn-square"
					aria-label="Close compose"
					onclick={handleClose}
				>
					<X size={16} />
				</button>
			</div>

			{#if !hasSmtp}
				<div class="px-6 py-8 text-center">
					<div
						class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-xl bg-warning/10 text-warning"
					>
						<AlertTriangle size={24} />
					</div>
					<h3 class="text-base font-bold text-base-content">Outgoing SMTP not configured</h3>
					<p class="mx-auto mt-1 mb-6 max-w-sm text-sm text-base-content/60">
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
					class="flex flex-col"
					onsubmit={(event) => {
						event.preventDefault();
						handleSubmit();
					}}
				>
					<!-- Header fields -->
					<div class="flex flex-col divide-y divide-[var(--rs-border)]">
						<div class="flex items-center gap-3 px-4">
							<span class="w-12 shrink-0 py-2 text-xs font-medium text-base-content/55">To</span>
							<input
								class="input input-sm input-ghost w-full rounded-none px-0 focus:bg-transparent"
								type="text"
								placeholder="To"
								aria-label="To"
								bind:value={draft.to}
								required
								autofocus
							/>
							<span class="flex shrink-0 gap-1">
								{#if !showCc}
									<button
										type="button"
										class="rounded px-1.5 py-0.5 text-xs text-base-content/50 hover:bg-base-200 hover:text-base-content"
										onclick={() => (showCc = true)}
									>
										Cc
									</button>
								{/if}
								{#if !showBcc}
									<button
										type="button"
										class="rounded px-1.5 py-0.5 text-xs text-base-content/50 hover:bg-base-200 hover:text-base-content"
										onclick={() => (showBcc = true)}
									>
										Bcc
									</button>
								{/if}
							</span>
						</div>
						{#if showCc}
							<div class="flex items-center gap-3 px-4">
								<span class="w-12 shrink-0 py-2 text-xs font-medium text-base-content/55">Cc</span>
								<input
									class="input input-sm input-ghost w-full rounded-none px-0 focus:bg-transparent"
									type="text"
									placeholder="Cc"
									aria-label="Cc"
									bind:value={draft.cc}
								/>
							</div>
						{/if}
						{#if showBcc}
							<div class="flex items-center gap-3 px-4">
								<span class="w-12 shrink-0 py-2 text-xs font-medium text-base-content/55">Bcc</span>
								<input
									class="input input-sm input-ghost w-full rounded-none px-0 focus:bg-transparent"
									type="text"
									placeholder="Bcc"
									aria-label="Bcc"
									bind:value={draft.bcc}
								/>
							</div>
						{/if}
						<div class="flex items-center gap-3 px-4">
							<span class="w-12 shrink-0 py-2 text-xs font-medium text-base-content/55">
								Subject
							</span>
							<input
								class="input input-sm input-ghost w-full rounded-none px-0 focus:bg-transparent"
								placeholder="Subject"
								aria-label="Subject"
								bind:value={draft.subject}
								required
							/>
						</div>
					</div>

					<!-- Attachments -->
					<div
						class="flex flex-wrap items-center gap-2 border-y border-[var(--rs-border)] bg-base-200/40 px-4 py-2"
					>
						<span class="flex items-center gap-1.5 text-xs font-semibold text-base-content/60">
							<Paperclip size={13} />
							Attachments
						</span>
						{#each attachedFiles as file}
							<span
								class="flex items-center gap-1 rounded-md border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] px-1.5 py-0.5 text-xs"
							>
								<span class="max-w-40 truncate">{file.name}</span>
								<button
									type="button"
									class="rounded p-0.5 text-base-content/45 hover:bg-error/10 hover:text-error"
									aria-label="Remove attachment {file.name}"
									onclick={() => removeAttachment(file.id)}
								>
									<X size={11} />
								</button>
							</span>
						{/each}
						<div class="ml-auto flex items-center gap-1.5">
							<select
								class="select select-xs select-bordered max-w-52"
								aria-label="Select workspace file to attach"
								bind:value={selectedFileIdToAdd}
							>
								<option value="">Link a file from workspace…</option>
								{#each files as file}
									{#if !draft.attachments.includes(file.id)}
										<option value={file.id}>{file.name}</option>
									{/if}
								{/each}
							</select>
							<button
								type="button"
								class="btn btn-xs btn-outline"
								onclick={addAttachment}
								disabled={!selectedFileIdToAdd}
							>
								Attach
							</button>
						</div>
					</div>

					<div class="px-4 py-3">
						<MailBodyEditor
							content={draft.body}
							placeholder="Message"
							onChange={(markdown) => (draft.body = markdown)}
						/>
					</div>

					<!-- Status + actions -->
					<div
						class="flex flex-wrap items-center gap-2 border-t border-[var(--rs-border)] px-4 py-2.5"
					>
						<div class="min-w-0 flex-1 text-xs">
							{#if saveError}
								<span class="text-error">{saveError}</span>
							{:else if saved && !changed}
								<span class="text-success">Saved draft</span>
							{:else if changed && draftId}
								<span class="text-base-content/50">Unsaved changes</span>
							{/if}
						</div>
						{#if draftId && onDiscard}
							<button
								type="button"
								class="btn btn-sm btn-ghost text-error"
								disabled={discarding || sending || saving}
								onclick={() => {
									if (confirm('Discard this draft permanently?')) onDiscard?.(draftId);
								}}
							>
								<Trash2 size={14} />
								{discarding ? 'Discarding...' : 'Discard'}
							</button>
						{/if}
						<button type="button" class="btn btn-sm btn-ghost" onclick={handleClose}>
							Cancel
						</button>
						<button
							type="button"
							class="btn btn-sm btn-outline gap-1.5"
							disabled={saving ||
								sending ||
								discarding ||
								(!draft.subject.trim() && !draft.body.trim())}
							onclick={handleSave}
						>
							<Save size={14} />
							<span
								>{saving
									? 'Saving...'
									: mode === 'draft-edit'
										? 'Save changes'
										: 'Save draft'}</span
							>
						</button>
						<button
							type="submit"
							class="btn btn-sm btn-primary gap-1.5"
							disabled={sending || saving || discarding}
						>
							<Send size={14} />
							<span>{sending ? 'Sending...' : saving ? 'Saving...' : 'Send'}</span>
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
