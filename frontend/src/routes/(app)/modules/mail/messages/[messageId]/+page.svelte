<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createMutation, createQuery } from '$lib/query-compat';
	import {
		mailApi,
		type MailAttachment,
		type MailLink,
		type MailMessage,
		type MailMessagePart,
		type MailSmtpSettings,
		type SaveDraftRequest
	} from '$lib/api/mail';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageSkeleton from '$lib/components/common/ModulePageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import FilePreviewModal from '$lib/components/modals/FilePreviewModal.svelte';
	import MailComposeModal from '$lib/components/modules/MailComposeModal.svelte';
	import { mailBodyText, quoteMailBody, uniqueMailAddresses } from '$lib/mail/compose';
	import { getModuleByKey } from '$lib/modules/registry';
	import { toastStore } from '$lib/stores/toast';
	import { sanitizeHtml } from '$lib/editor/adapter/security';
	import { listAllFiles } from '$lib/api/files';
	import {
		ArrowLeft,
		Download,
		Paperclip,
		Link2,
		Trash2,
		Reply,
		Forward,
		ReplyAll
	} from 'lucide-svelte';

	let messageId = $derived($page.params.messageId);
	let mailEnabled = $derived(getModuleByKey('mail')?.enabled !== false);

	const messageQuery = createQuery<MailMessage>({
		queryKey: ['mail-message', null],
		queryFn: () => Promise.reject(new Error('Missing message id')),
		enabled: false
	});

	const partsQuery = createQuery<MailMessagePart[]>({
		queryKey: ['mail-message-parts', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});

	const attachmentsQuery = createQuery<MailAttachment[]>({
		queryKey: ['mail-message-attachments', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});

	const linksQuery = createQuery<MailLink[]>({
		queryKey: ['mail-message-links', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});

	$effect(() => {
		const id = messageId ?? null;
		messageQuery.setOptions({
			queryKey: ['mail-message', id],
			queryFn: () => mailApi.getMessage(messageId!),
			enabled: mailEnabled && !!messageId
		});
		partsQuery.setOptions({
			queryKey: ['mail-message-parts', id],
			queryFn: () => mailApi.listParts(messageId!),
			enabled: mailEnabled && !!messageId
		});
		attachmentsQuery.setOptions({
			queryKey: ['mail-message-attachments', id],
			queryFn: () => mailApi.listAttachments(messageId!),
			enabled: mailEnabled && !!messageId
		});
		linksQuery.setOptions({
			queryKey: ['mail-message-links', id],
			queryFn: () => mailApi.listLinks(messageId!),
			enabled: mailEnabled && !!messageId
		});
	});

	let bodyContent = $derived.by(async () => {
		const parts = $partsQuery.data ?? [];
		const htmlPart = parts.find((p) => p.is_body && p.content_type === 'text/html');
		const textPart = parts.find((p) => p.is_body && p.content_type === 'text/plain');
		const part = htmlPart ?? textPart;
		if (!part) return { type: 'empty' as const, content: '' };
		const raw = await mailApi.getPartContent(messageId!, part.id);
		if (htmlPart) {
			return { type: 'html' as const, content: sanitizeHtml(raw) };
		}
		return { type: 'text' as const, content: raw };
	});

	let previewAttachment = $state<MailAttachment | null>(null);
	type MailLinkTargetType =
		'file' | 'folder' | 'note' | 'meeting' | 'kanban_board' | 'kanban_card' | 'mail_message';
	const linkTargetTypes: { value: MailLinkTargetType; label: string }[] = [
		{ value: 'file', label: 'File' },
		{ value: 'folder', label: 'Folder' },
		{ value: 'note', label: 'Note' },
		{ value: 'meeting', label: 'Meeting' },
		{ value: 'kanban_board', label: 'Kanban board' },
		{ value: 'kanban_card', label: 'Kanban card' },
		{ value: 'mail_message', label: 'Mail message' }
	];
	let linkTargetType = $state<MailLinkTargetType>('file');
	let linkTargetId = $state('');
	let composeOpen = $state(false);
	let composeTo = $state('');
	let composeCc = $state('');
	let composeBcc = $state('');
	let composeSubject = $state('');
	let composeBody = $state('');
	let composeAttachments = $state<string[]>([]);
	let composeMode = $state<'new' | 'reply' | 'reply-all' | 'forward'>('new');
	let composeDraftId = $state<string | null>(null);
	let composeSaveError = $state('');
	let smtpSettings = $state<MailSmtpSettings | null>(null);

	const accountsQuery = createQuery({
		queryKey: ['mail-accounts'],
		queryFn: () => mailApi.listAccounts()
	});

	const filesQuery = createQuery({
		queryKey: ['mail-link-files'],
		queryFn: () => listAllFiles()
	});

	$effect(() => {
		const accountId = $messageQuery.data?.account_id ?? $accountsQuery.data?.[0]?.id;
		smtpSettings = null;
		if (!accountId) return;
		mailApi
			.getSmtpSettings(accountId)
			.then((settings) => (smtpSettings = settings))
			.catch(() => (smtpSettings = null));
	});

	const createLinkMutation = createMutation({
		mutationFn: () =>
			mailApi.createLink(messageId!, {
				target_type: linkTargetType,
				target_id: linkTargetId.trim()
			}),
		onSuccess: async () => {
			linkTargetId = '';
			await linksQuery.refetch();
			toastStore.show('Mail link added', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Link failed', 'error')
	});

	const deleteLinkMutation = createMutation({
		mutationFn: (linkId: string) => mailApi.deleteLink(messageId!, linkId),
		onSuccess: async () => {
			await linksQuery.refetch();
			toastStore.show('Mail link removed', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Unlink failed', 'error')
	});

	const sendMutation = createMutation({
		mutationFn: async (input: any) => {
			const message = $messageQuery.data;
			const accountId = message?.account_id || $accountsQuery.data?.[0]?.id;
			if (!accountId) {
				throw new Error('No mail account configured to send from.');
			}
			if (composeMode === 'reply') {
				return mailApi.replyMail(accountId, input);
			} else if (composeMode === 'reply-all') {
				return mailApi.replyAllMail(accountId, input);
			} else if (composeMode === 'forward') {
				return mailApi.forwardMail(accountId, input);
			} else {
				return mailApi.sendOutboundMail(accountId, input);
			}
		},
		onSuccess: async (result) => {
			composeOpen = false;
			if (composeDraftId) {
				const accountId = $messageQuery.data?.account_id || $accountsQuery.data?.[0]?.id;
				const draftId = composeDraftId;
				composeDraftId = null;
				if (accountId) {
					try {
						await mailApi.discardDraft(accountId, draftId);
					} catch {
						toastStore.show('Sent, but the saved draft could not be removed', 'info');
					}
				}
			}
			toastStore.show(
				!result.stored
					? 'Mail sent, but the RustShare copy could not be saved'
					: result.append_failed
						? 'Mail sent, but not saved to the Sent folder'
						: 'Mail sent',
				!result.stored || result.append_failed ? 'info' : 'success'
			);
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Send failed', 'error')
	});

	const saveDraftMutation = createMutation({
		mutationFn: async ({
			message,
			draftId
		}: {
			message: SaveDraftRequest;
			draftId: string | null;
		}) => {
			const current = $messageQuery.data;
			const accountId = current?.account_id || $accountsQuery.data?.[0]?.id;
			if (!accountId) throw new Error('No mail account configured to save a draft.');
			return draftId
				? mailApi.updateDraft(accountId, draftId, message)
				: mailApi.saveDraft(accountId, message);
		},
		onSuccess: (draft) => {
			composeDraftId = draft.id;
			composeSaveError = '';
			toastStore.show('Draft saved', 'success');
		},
		onError: (error) => {
			composeSaveError = error instanceof Error ? error.message : 'Draft save failed';
			toastStore.show(composeSaveError, 'error');
		}
	});

	function formatAddresses(value: unknown): string {
		return addressStrings(value).join(', ');
	}

	function addressStrings(value: unknown): string[] {
		if (!Array.isArray(value)) return value ? [String(value)] : [];
		return value.map((item) => (typeof item === 'string' ? item : item?.address)).filter(Boolean);
	}

	function prefixedSubject(prefix: string, subject: string | null): string {
		const value = subject || '(no subject)';
		return value.toLowerCase().startsWith(prefix.toLowerCase()) ? value : `${prefix} ${value}`;
	}

	async function openReply(message: MailMessage) {
		const original = mailBodyText(await bodyContent);
		composeMode = 'reply';
		composeDraftId = null;
		composeSaveError = '';
		composeTo = message.from_address ?? '';
		composeCc = '';
		composeBcc = '';
		composeSubject = prefixedSubject('Re:', message.subject);
		composeBody = `\n\nOn ${
			message.sent_at ? new Date(message.sent_at).toLocaleString() : 'unknown date'
		}, ${message.from_name || message.from_address || 'sender'} wrote:\n${quoteMailBody(original)}`;
		composeAttachments = [];
		composeOpen = true;
	}

	async function openReplyAll(message: MailMessage) {
		const original = mailBodyText(await bodyContent);
		const account = ($accountsQuery.data ?? []).find((item) => item.id === message.account_id);
		const excluded = account?.username ? [account.username] : [];
		const to = uniqueMailAddresses(
			[message.from_address ?? '', ...addressStrings(message.to_addresses)],
			excluded
		);
		const cc = uniqueMailAddresses(addressStrings(message.cc_addresses), [...excluded, ...to]);
		composeMode = 'reply-all';
		composeDraftId = null;
		composeSaveError = '';
		composeTo = to.join(', ');
		composeCc = cc.join(', ');
		composeBcc = '';
		composeSubject = prefixedSubject('Re:', message.subject);
		composeBody = `\n\nOn ${
			message.sent_at ? new Date(message.sent_at).toLocaleString() : 'unknown date'
		}, ${message.from_name || message.from_address || 'sender'} wrote:\n${quoteMailBody(original)}`;
		composeAttachments = [];
		composeOpen = true;
	}

	async function openForward(message: MailMessage) {
		const original = mailBodyText(await bodyContent);
		composeMode = 'forward';
		composeDraftId = null;
		composeSaveError = '';
		composeTo = '';
		composeCc = '';
		composeBcc = '';
		composeSubject = prefixedSubject('Fwd:', message.subject);
		composeBody = `\n\n---------- Forwarded message ----------\nFrom: ${formatAddresses(
			[message.from_name, message.from_address].filter(Boolean)
		)}\nDate: ${
			message.sent_at ? new Date(message.sent_at).toLocaleString() : 'Unknown'
		}\nSubject: ${message.subject || '(no subject)'}\n\n${original}`;

		// Copy attachments if any; await the query so early clicks cannot drop them
		const attachments = (await $attachmentsQuery.refetch()).data ?? [];
		composeAttachments = attachments.map((a) => a.file_id).filter((id): id is string => !!id);
		if (composeAttachments.length !== attachments.length) {
			toastStore.show('Some attachments are unavailable and were not added to the forward', 'info');
		}

		composeOpen = true;
	}
</script>

{#if !mailEnabled}
	<ErrorState
		title="Mail is disabled"
		message="Enable the Mail module before opening imported messages."
	/>
{:else if $messageQuery.isLoading || $partsQuery.isLoading}
	<ModulePageSkeleton />
{:else if $messageQuery.isError}
	<ErrorState
		title="Failed to load message"
		message={$messageQuery.error?.message || 'Unknown error'}
		onRetry={() => $messageQuery.refetch()}
	/>
{:else if $messageQuery.data}
	{@const message = $messageQuery.data}
	<ModulePageShell
		title={message.subject || '(no subject)'}
		subtitle={message.from_name || message.from_address || 'Unknown sender'}
	>
		<div slot="secondaryActions">
			<button class="btn gap-2 btn-outline btn-sm" onclick={() => openReply(message)}>
				<Reply size={14} />
				<span>Reply</span>
			</button>
			<button class="btn gap-2 btn-outline btn-sm" onclick={() => openReplyAll(message)}>
				<ReplyAll size={14} />
				<span>Reply all</span>
			</button>
			<button class="btn gap-2 btn-outline btn-sm" onclick={() => openForward(message)}>
				<Forward size={14} />
				<span>Forward</span>
			</button>
			<button class="btn gap-2 btn-outline btn-sm" onclick={() => goto('/modules/mail')}>
				<ArrowLeft size={14} />
				<span>Back</span>
			</button>
			<a href={mailApi.downloadSourceUrl(messageId!)} download class="btn gap-2 btn-outline btn-sm">
				<Download size={14} />
				<span>Download .eml</span>
			</a>
		</div>

		<div class="flex flex-col gap-6">
			<div class="rounded-xl border border-base-300/70 bg-base-100 p-4 shadow-sm">
				<div class="grid grid-cols-1 gap-2 text-sm">
					<div>
						<span class="text-base-content/55">From:</span>
						{formatAddresses([message.from_name, message.from_address].filter(Boolean))}
					</div>
					<div>
						<span class="text-base-content/55">To:</span>
						{formatAddresses(message.to_addresses)}
					</div>
					{#if message.cc_addresses && JSON.stringify(message.cc_addresses) !== '[]'}
						<div>
							<span class="text-base-content/55">Cc:</span>
							{formatAddresses(message.cc_addresses)}
						</div>
					{/if}
					<div>
						<span class="text-base-content/55">Date:</span>
						{message.sent_at ? new Date(message.sent_at).toLocaleString() : 'Unknown'}
					</div>
				</div>
			</div>

			<div class="rounded-xl border border-base-300/70 bg-base-100 p-4 shadow-sm">
				{#await bodyContent}
					<ModulePageSkeleton />
				{:then body}
					{#if body.type === 'html'}
						<div class="prose max-w-none">{@html body.content}</div>
					{:else if body.type === 'text'}
						<pre class="whitespace-pre-wrap font-mono text-sm">{body.content}</pre>
					{:else}
						<EmptyState
							icon="📄"
							title="No readable body"
							description="This message has no text or HTML body part."
						/>
					{/if}
				{:catch error}
					<ErrorState title="Failed to load body" message={error?.message || 'Unknown error'} />
				{/await}
			</div>

			{#if $attachmentsQuery.data && $attachmentsQuery.data.length > 0}
				<div class="rounded-xl border border-base-300/70 bg-base-100 p-4 shadow-sm">
					<h3 class="mb-3 flex items-center gap-2 font-semibold">
						<Paperclip size={16} /> Attachments
					</h3>
					<div class="flex flex-wrap gap-2">
						{#each $attachmentsQuery.data as attachment}
							<div class="max-w-full rounded-lg border border-base-300 px-3 py-2 text-sm">
								<div class="flex items-center gap-2">
									<span class="truncate font-medium">{attachment.filename}</span>
									{#if attachment.file_id}
										<button
											type="button"
											class="btn btn-outline btn-xs"
											onclick={() => (previewAttachment = attachment)}
										>
											Open file
										</button>
									{:else}
										<span class="badge badge-ghost badge-sm">mail-only</span>
									{/if}
								</div>
								<div class="mt-1 truncate text-xs text-base-content/55">
									{attachment.mime_type ?? 'application/octet-stream'} · {Number(
										attachment.size_bytes ?? 0
									).toLocaleString()} bytes
								</div>
							</div>
						{/each}
					</div>
				</div>
			{/if}

			<div class="rounded-xl border border-base-300/70 bg-base-100 p-4 shadow-sm">
				<h3 class="mb-3 flex items-center gap-2 font-semibold"><Link2 size={16} /> Links</h3>
				<form
					class="mb-4 grid grid-cols-1 gap-2 md:grid-cols-[minmax(0,12rem)_minmax(0,1fr)_auto]"
					onsubmit={(event) => {
						event.preventDefault();
						createLinkMutation.mutate();
					}}
				>
					<select
						class="select select-sm select-bordered"
						bind:value={linkTargetType}
						onchange={() => (linkTargetId = '')}
					>
						{#each linkTargetTypes as targetType}
							<option value={targetType.value}>{targetType.label}</option>
						{/each}
					</select>
					{#if linkTargetType === 'file'}
						<select class="select select-sm select-bordered" bind:value={linkTargetId} required>
							<option value="">Select a file</option>
							{#each $filesQuery.data ?? [] as file}
								<option value={file.id}>{file.name}</option>
							{/each}
						</select>
					{:else}
						<input
							class="input input-sm input-bordered"
							bind:value={linkTargetId}
							placeholder="Artifact UUID"
							required
						/>
					{/if}
					<button
						class="btn btn-sm btn-primary"
						type="submit"
						disabled={$createLinkMutation.isPending}
					>
						Add link
					</button>
				</form>
				{#if $linksQuery.isLoading}
					<ModulePageSkeleton />
				{:else if $linksQuery.isError}
					<ErrorState
						title="Failed to load links"
						message={$linksQuery.error?.message || 'Unknown error'}
						onRetry={() => linksQuery.refetch()}
					/>
				{:else if ($linksQuery.data ?? []).length === 0}
					<p class="text-sm text-base-content/60">No links yet.</p>
				{:else}
					<div class="flex flex-col gap-2">
						{#each $linksQuery.data ?? [] as link}
							<div
								class="flex items-center justify-between gap-3 rounded-lg border border-base-300 p-3"
							>
								<div class="min-w-0">
									<div class="text-sm font-medium">
										{link.target_type === 'file'
											? (($filesQuery.data ?? []).find((file) => file.id === link.target_id)
													?.name ?? 'Linked file')
											: (linkTargetTypes.find((type) => type.value === link.target_type)?.label ??
												link.target_type)}
									</div>
									<div class="truncate font-mono text-xs text-base-content/60">
										{link.target_id}
									</div>
								</div>
								<button
									type="button"
									class="btn btn-error btn-xs btn-outline"
									onclick={() => deleteLinkMutation.mutate(link.id)}
									aria-label="Remove link"
								>
									<Trash2 size={13} />
								</button>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		</div>
	</ModulePageShell>

	<FilePreviewModal
		open={previewAttachment !== null}
		file={previewAttachment
			? {
					id: previewAttachment.file_id ?? previewAttachment.id,
					name: previewAttachment.filename,
					mime_type: previewAttachment.mime_type ?? 'application/octet-stream',
					size: Number(previewAttachment.size_bytes ?? 0)
				}
			: null}
		onClose={() => (previewAttachment = null)}
	/>

	<MailComposeModal
		open={composeOpen}
		draftId={composeDraftId}
		initialTo={composeTo}
		initialCc={composeCc}
		initialBcc={composeBcc}
		initialSubject={composeSubject}
		initialBody={composeBody}
		initialAttachments={composeAttachments}
		inReplyToMsgId={messageId}
		mode={composeMode}
		sending={$sendMutation.isPending}
		saving={$saveDraftMutation.isPending}
		hasSmtp={!!smtpSettings && smtpSettings.is_enabled}
		saveError={composeSaveError}
		onClose={() => (composeOpen = false)}
		onSend={(message) => sendMutation.mutate(message)}
		onSave={(message, draftId) =>
			saveDraftMutation.mutateAsync({ message, draftId }).then(() => undefined)}
	/>
{/if}
