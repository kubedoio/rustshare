<script lang="ts">
	import { goto } from '$app/navigation';
	import { createMutation, createQuery } from '$lib/query-compat';
	import {
		mailApi,
		type MailAttachment,
		type MailLink,
		type MailMessage,
		type MailMessagePart
	} from '$lib/api/mail';
	import { listAllFiles } from '$lib/api/files';
	import { sanitizeHtml } from '$lib/editor/adapter/security';
	import { mailBodyText, quoteMailBody, uniqueMailAddresses } from '$lib/mail/compose';
	import { toastStore } from '$lib/stores/toast';
	import ModulePageSkeleton from '$lib/components/common/ModulePageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import FilePreviewModal from '$lib/components/modals/FilePreviewModal.svelte';
	import {
		ArrowLeft,
		Archive,
		Copy,
		Download,
		Eye,
		EyeOff,
		ExternalLink,
		Forward,
		HardDriveDownload,
		Link2,
		MailOpen,
		Paperclip,
		Reply,
		ReplyAll,
		Trash,
		Trash2
	} from 'lucide-svelte';
	import type { ViewerTarget } from './mail-types';
	import {
		formatMailAddresses,
		formatMailBytes,
		formatSourceMode,
		mailAddressStrings
	} from './mail-types';

	export type ComposeRequest = {
		mode: 'new' | 'reply' | 'reply-all' | 'forward';
		to: string;
		cc: string;
		bcc: string;
		subject: string;
		body: string;
		attachments: string[];
		inReplyTo: string | null;
	};

	let {
		target,
		accountId,
		accountUsername,
		archiveFolderName,
		trashFolderName,
		uidvalidity,
		onBack,
		onCompose,
		onImapAction,
		onImportUid
	}: {
		target: ViewerTarget;
		accountId: string | null;
		accountUsername: string | null;
		archiveFolderName: string | undefined;
		trashFolderName: string | undefined;
		uidvalidity: number | null;
		onBack?: (() => void) | null;
		onCompose: (request: ComposeRequest) => void;
		onImapAction: (action: 'read' | 'unread' | 'archive' | 'trash' | 'delete', uid: number) => void;
		onImportUid: (uid: number) => void;
	} = $props();

	let storedId = $derived(target?.kind === 'stored' ? target.id : null);

	const messageQuery = createQuery<MailMessage>({
		queryKey: ['mail-viewer-message', null],
		queryFn: () => Promise.reject(new Error('Missing message id')),
		enabled: false
	});
	const partsQuery = createQuery<MailMessagePart[]>({
		queryKey: ['mail-viewer-parts', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});
	const attachmentsQuery = createQuery<MailAttachment[]>({
		queryKey: ['mail-viewer-attachments', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});
	const linksQuery = createQuery<MailLink[]>({
		queryKey: ['mail-viewer-links', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});
	const filesQuery = createQuery({
		queryKey: ['mail-link-files'],
		queryFn: () => listAllFiles()
	});

	$effect(() => {
		const id = storedId;
		messageQuery.setOptions({
			queryKey: ['mail-viewer-message', id],
			queryFn: () => mailApi.getMessage(id!),
			enabled: !!id
		});
		partsQuery.setOptions({
			queryKey: ['mail-viewer-parts', id],
			queryFn: () => mailApi.listParts(id!),
			enabled: !!id
		});
		attachmentsQuery.setOptions({
			queryKey: ['mail-viewer-attachments', id],
			queryFn: () => mailApi.listAttachments(id!),
			enabled: !!id
		});
		linksQuery.setOptions({
			queryKey: ['mail-viewer-links', id],
			queryFn: () => mailApi.listLinks(id!),
			enabled: !!id
		});
	});

	let bodyContent = $derived.by(async () => {
		const parts = $partsQuery.data ?? [];
		const htmlPart = parts.find((p) => p.is_body && p.content_type === 'text/html');
		const textPart = parts.find((p) => p.is_body && p.content_type === 'text/plain');
		const part = htmlPart ?? textPart;
		if (!part || !storedId) return { type: 'empty' as const, content: '' };
		const raw = await mailApi.getPartContent(storedId, part.id);
		if (htmlPart) return { type: 'html' as const, content: sanitizeHtml(raw) };
		return { type: 'text' as const, content: raw };
	});

	let storedMessage = $derived($messageQuery.data ?? null);
	let previewAttachment = $state<MailAttachment | null>(null);
	let detailsOpen = $state(false);

	// Reference (link) editor state
	type LinkTargetType =
		'file' | 'folder' | 'note' | 'meeting' | 'kanban_board' | 'kanban_card' | 'mail_message';
	const linkTargetTypes: { value: LinkTargetType; label: string }[] = [
		{ value: 'file', label: 'File' },
		{ value: 'folder', label: 'Folder' },
		{ value: 'note', label: 'Note' },
		{ value: 'meeting', label: 'Meeting' },
		{ value: 'kanban_board', label: 'Kanban board' },
		{ value: 'kanban_card', label: 'Kanban card' },
		{ value: 'mail_message', label: 'Mail message' }
	];
	let linkEditorOpen = $state(false);
	let linkTargetType = $state<LinkTargetType>('file');
	let linkTargetId = $state('');

	$effect(() => {
		// Reset per-message UI state when the selection changes
		void storedId;
		detailsOpen = false;
		linkEditorOpen = false;
		linkTargetId = '';
		previewAttachment = null;
	});

	const createLinkMutation = createMutation({
		mutationFn: () =>
			mailApi.createLink(storedId!, {
				target_type: linkTargetType,
				target_id: linkTargetId.trim()
			}),
		onSuccess: async () => {
			linkTargetId = '';
			await $linksQuery.refetch();
			toastStore.show('Reference added', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Reference failed', 'error')
	});

	const deleteLinkMutation = createMutation({
		mutationFn: (linkId: string) => mailApi.deleteLink(storedId!, linkId),
		onSuccess: async () => {
			await $linksQuery.refetch();
			toastStore.show('Reference removed', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Unlink failed', 'error')
	});

	function prefixedSubject(prefix: string, subject: string | null): string {
		const value = subject || '(no subject)';
		return value.toLowerCase().startsWith(prefix.toLowerCase()) ? value : `${prefix} ${value}`;
	}

	async function startReply(message: MailMessage) {
		const original = mailBodyText(await bodyContent);
		onCompose({
			mode: 'reply',
			to: message.from_address ?? '',
			cc: '',
			bcc: '',
			subject: prefixedSubject('Re:', message.subject),
			body: `\n\nOn ${
				message.sent_at ? new Date(message.sent_at).toLocaleString() : 'unknown date'
			}, ${message.from_name || message.from_address || 'sender'} wrote:\n${quoteMailBody(original)}`,
			attachments: [],
			inReplyTo: storedId
		});
	}

	async function startReplyAll(message: MailMessage) {
		const original = mailBodyText(await bodyContent);
		const excluded = accountUsername ? [accountUsername] : [];
		const to = uniqueMailAddresses(
			[message.from_address ?? '', ...mailAddressStrings(message.to_addresses)],
			excluded
		);
		const cc = uniqueMailAddresses(mailAddressStrings(message.cc_addresses), [...excluded, ...to]);
		onCompose({
			mode: 'reply-all',
			to: to.join(', '),
			cc: cc.join(', '),
			bcc: '',
			subject: prefixedSubject('Re:', message.subject),
			body: `\n\nOn ${
				message.sent_at ? new Date(message.sent_at).toLocaleString() : 'unknown date'
			}, ${message.from_name || message.from_address || 'sender'} wrote:\n${quoteMailBody(original)}`,
			attachments: [],
			inReplyTo: storedId
		});
	}

	async function startForward(message: MailMessage) {
		const original = mailBodyText(await bodyContent);
		const attachments = (await $attachmentsQuery.refetch()).data ?? [];
		const fileIds = attachments.map((a) => a.file_id).filter((id): id is string => !!id);
		if (fileIds.length !== attachments.length) {
			toastStore.show('Some attachments are unavailable and were not added to the forward', 'info');
		}
		onCompose({
			mode: 'forward',
			to: '',
			cc: '',
			bcc: '',
			subject: prefixedSubject('Fwd:', message.subject),
			body: `\n\n---------- Forwarded message ----------\nFrom: ${formatMailAddresses(
				[message.from_name, message.from_address].filter(Boolean)
			)}\nDate: ${
				message.sent_at ? new Date(message.sent_at).toLocaleString() : 'Unknown'
			}\nSubject: ${message.subject || '(no subject)'}\n\n${original}`,
			attachments: fileIds,
			inReplyTo: storedId
		});
	}

	async function copyMessageLink(id: string) {
		const url = `${window.location.origin}/modules/mail/messages/${id}`;
		try {
			await navigator.clipboard.writeText(url);
			toastStore.show('Message link copied', 'success');
		} catch {
			toastStore.show(url, 'info', { duration: 8000 });
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	{#if !target}
		<div class="flex h-full flex-col items-center justify-center px-6 text-center">
			<MailOpen size={26} class="text-base-content/20" />
			<p class="mt-3 text-sm font-semibold text-base-content/70">Select a message</p>
			<p class="mt-1 max-w-56 text-xs text-base-content/50">
				Choose a message from the list to read or reference it.
			</p>
		</div>
	{:else if target.kind === 'imap'}
		{@const message = target.message}
		<!-- Remote IMAP summary: full content requires import -->
		<div
			class="flex items-center gap-2 border-b border-[var(--rs-border)] px-3 py-1.5"
			role="toolbar"
			aria-label="Message actions"
		>
			{#if onBack}
				<button
					type="button"
					class="btn btn-xs btn-ghost btn-square lg:hidden"
					aria-label="Back to message list"
					onclick={onBack}
				>
					<ArrowLeft size={14} />
				</button>
			{/if}
			<button
				type="button"
				class="btn btn-xs btn-primary gap-1"
				onclick={() => onImportUid(message.uid)}
			>
				<HardDriveDownload size={12} /> Save to workspace
			</button>
			<button
				type="button"
				class="btn btn-xs btn-ghost btn-square"
				title={message.is_seen ? 'Mark as unread' : 'Mark as read'}
				aria-label={message.is_seen ? 'Mark as unread' : 'Mark as read'}
				onclick={() => onImapAction(message.is_seen ? 'unread' : 'read', message.uid)}
			>
				{#if message.is_seen}<EyeOff size={13} />{:else}<Eye size={13} />{/if}
			</button>
			<button
				type="button"
				class="btn btn-xs btn-ghost btn-square"
				disabled={!archiveFolderName}
				title={archiveFolderName ? 'Archive' : 'No archive folder is configured'}
				aria-label={archiveFolderName ? 'Archive' : 'No archive folder is configured'}
				onclick={() => onImapAction('archive', message.uid)}
			>
				<Archive size={13} />
			</button>
			<button
				type="button"
				class="btn btn-xs btn-ghost btn-square"
				disabled={!trashFolderName}
				title={trashFolderName ? 'Move to trash' : 'No trash folder is configured'}
				aria-label={trashFolderName ? 'Move to trash' : 'No trash folder is configured'}
				onclick={() => onImapAction('trash', message.uid)}
			>
				<Trash2 size={13} />
			</button>
			<button
				type="button"
				class="btn btn-xs btn-ghost btn-square text-error"
				title="Delete permanently"
				aria-label="Delete permanently"
				onclick={() => onImapAction('delete', message.uid)}
			>
				<Trash size={13} />
			</button>
		</div>
		<div class="min-h-0 flex-1 overflow-y-auto p-4">
			<h2 class="text-base font-semibold text-base-content">{message.subject || '(no subject)'}</h2>
			<p class="mt-1 text-sm text-base-content/70">
				{message.from_name || message.from_address || 'Unknown sender'}
			</p>
			<p class="mt-0.5 text-xs text-base-content/50">
				{message.sent_at ? new Date(message.sent_at).toLocaleString() : 'No date'} · {formatMailBytes(
					message.size_bytes
				)}
			</p>
			<div class="mt-4 rounded-md border border-[var(--rs-border)] bg-base-200/40 p-3">
				<p class="text-xs text-base-content/60">
					This is a remote IMAP message. Save it to the workspace to read the full content,
					attachments, and to reference it in RustShare.
				</p>
			</div>
		</div>
	{:else if $messageQuery.isLoading || $partsQuery.isLoading}
		<ModulePageSkeleton />
	{:else if $messageQuery.isError}
		<ErrorState
			title="Failed to load message"
			message={$messageQuery.error?.message || 'Unknown error'}
			onRetry={() => $messageQuery.refetch()}
		/>
	{:else if storedMessage}
		{@const message = storedMessage}
		<!-- Action toolbar -->
		<div
			class="flex flex-wrap items-center gap-1 border-b border-[var(--rs-border)] px-3 py-1.5"
			role="toolbar"
			aria-label="Message actions"
		>
			{#if onBack}
				<button
					type="button"
					class="btn btn-xs btn-ghost btn-square lg:hidden"
					aria-label="Back to message list"
					onclick={onBack}
				>
					<ArrowLeft size={14} />
				</button>
			{/if}
			<button
				type="button"
				class="btn btn-xs btn-outline gap-1"
				onclick={() => startReply(message)}
			>
				<Reply size={12} /> Reply
			</button>
			<button
				type="button"
				class="btn btn-xs btn-outline gap-1"
				onclick={() => startReplyAll(message)}
			>
				<ReplyAll size={12} /> <span class="hidden sm:inline">Reply all</span>
			</button>
			<button
				type="button"
				class="btn btn-xs btn-outline gap-1"
				onclick={() => startForward(message)}
			>
				<Forward size={12} /> <span class="hidden sm:inline">Forward</span>
			</button>
			<div class="mx-1 h-4 w-px bg-[var(--rs-border)]" aria-hidden="true"></div>
			<button
				type="button"
				class="btn btn-xs btn-ghost gap-1 text-brand-600"
				title="Reference this message in a file, note, or board"
				onclick={() => (linkEditorOpen = !linkEditorOpen)}
			>
				<Link2 size={12} /> Reference
			</button>
			<button
				type="button"
				class="btn btn-xs btn-ghost btn-square"
				title="Copy message link"
				aria-label="Copy message link"
				onclick={() => copyMessageLink(message.id)}
			>
				<Copy size={13} />
			</button>
			<a
				class="btn btn-xs btn-ghost btn-square"
				title="Download .eml"
				aria-label="Download .eml"
				href={mailApi.downloadSourceUrl(message.id)}
				download
			>
				<Download size={13} />
			</a>
			<button
				type="button"
				class="btn btn-xs btn-ghost btn-square"
				title="Open full page"
				aria-label="Open full page"
				onclick={() => goto(`/modules/mail/messages/${message.id}`)}
			>
				<ExternalLink size={13} />
			</button>
		</div>

		<!-- Message content -->
		<div class="min-h-0 flex-1 overflow-y-auto">
			<div class="border-b border-[var(--rs-border)] px-4 py-3">
				<h2 class="text-base font-semibold text-base-content">
					{message.subject || '(no subject)'}
				</h2>
				<div class="mt-1 flex flex-wrap items-baseline gap-x-2 text-sm">
					<span class="font-medium text-base-content/85">
						{message.from_name || message.from_address || 'Unknown sender'}
					</span>
					{#if message.from_name && message.from_address}
						<span class="text-xs text-base-content/50">&lt;{message.from_address}&gt;</span>
					{/if}
					<span class="text-xs text-base-content/50">
						{message.sent_at
							? new Date(message.sent_at).toLocaleString()
							: `imported ${new Date(message.imported_at).toLocaleString()}`}
					</span>
				</div>
				<p class="mt-0.5 truncate text-xs text-base-content/55">
					To: {formatMailAddresses(message.to_addresses) || '(no recipients)'}
					{#if mailAddressStrings(message.cc_addresses).length > 0}
						· Cc: {formatMailAddresses(message.cc_addresses)}
					{/if}
				</p>
				<button
					type="button"
					class="mt-1 text-2xs text-base-content/45 underline-offset-2 hover:underline"
					aria-expanded={detailsOpen}
					onclick={() => (detailsOpen = !detailsOpen)}
				>
					{detailsOpen ? 'Hide details' : 'Details'}
				</button>
				{#if detailsOpen}
					<dl class="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-2xs text-base-content/60">
						<dt class="font-medium">Source</dt>
						<dd>{formatSourceMode(message.source_mode)}</dd>
						<dt class="font-medium">Size</dt>
						<dd>{formatMailBytes(message.size_bytes)}</dd>
						<dt class="font-medium">Imported</dt>
						<dd>{new Date(message.imported_at).toLocaleString()}</dd>
						<dt class="font-medium">Message ID</dt>
						<dd class="font-mono">{message.id}</dd>
					</dl>
				{/if}
			</div>

			<div class="px-4 py-3">
				{#await bodyContent}
					<div class="flex flex-col gap-2" aria-label="Loading message body">
						<div class="skeleton h-3.5 w-full"></div>
						<div class="skeleton h-3.5 w-11/12"></div>
						<div class="skeleton h-3.5 w-4/5"></div>
						<div class="skeleton h-3.5 w-3/5"></div>
					</div>
				{:then body}
					{#if body.type === 'html'}
						<div class="prose prose-sm max-w-[65ch]">{@html body.content}</div>
					{:else if body.type === 'text'}
						<pre
							class="max-w-[80ch] font-sans text-sm whitespace-pre-wrap text-base-content/90">{body.content}</pre>
					{:else}
						<p class="text-sm text-base-content/50">This message has no text or HTML body part.</p>
					{/if}
				{:catch error}
					<p class="text-sm text-error" role="alert">
						Failed to load body: {error?.message || 'Unknown error'}
					</p>
				{/await}
			</div>

			{#if $attachmentsQuery.data && $attachmentsQuery.data.length > 0}
				<div class="border-t border-[var(--rs-border)] px-4 py-3">
					<h3 class="mb-2 flex items-center gap-1.5 text-xs font-semibold text-base-content/70">
						<Paperclip size={13} /> Attachments ({$attachmentsQuery.data.length})
					</h3>
					<div class="flex flex-wrap gap-2">
						{#each $attachmentsQuery.data as attachment}
							<div
								class="flex items-center gap-2 rounded-md border border-[var(--rs-border)] px-2.5 py-1.5 text-xs"
							>
								<span class="max-w-48 truncate font-medium">{attachment.filename}</span>
								<span class="text-base-content/45">
									{formatMailBytes(attachment.size_bytes)}
								</span>
								{#if attachment.file_id}
									<button
										type="button"
										class="btn btn-xs btn-outline"
										onclick={() => (previewAttachment = attachment)}
									>
										Open
									</button>
								{:else}
									<span class="text-base-content/40">mail-only</span>
								{/if}
							</div>
						{/each}
					</div>
				</div>
			{/if}

			{#if linkEditorOpen || ($linksQuery.data ?? []).length > 0}
				<div class="border-t border-[var(--rs-border)] px-4 py-3">
					<h3 class="mb-2 flex items-center gap-1.5 text-xs font-semibold text-base-content/70">
						<Link2 size={13} /> RustShare references
					</h3>
					{#if linkEditorOpen}
						<form
							class="mb-3 grid grid-cols-1 gap-2 sm:grid-cols-[minmax(0,10rem)_minmax(0,1fr)_auto]"
							onsubmit={(event) => {
								event.preventDefault();
								createLinkMutation.mutate();
							}}
						>
							<select
								class="select select-xs select-bordered"
								aria-label="Reference target type"
								bind:value={linkTargetType}
								onchange={() => (linkTargetId = '')}
							>
								{#each linkTargetTypes as targetType}
									<option value={targetType.value}>{targetType.label}</option>
								{/each}
							</select>
							{#if linkTargetType === 'file'}
								<select
									class="select select-xs select-bordered"
									aria-label="Reference file"
									bind:value={linkTargetId}
									required
								>
									<option value="">Select a file</option>
									{#each $filesQuery.data ?? [] as file}
										<option value={file.id}>{file.name}</option>
									{/each}
								</select>
							{:else}
								<input
									class="input input-xs input-bordered"
									bind:value={linkTargetId}
									placeholder="Artifact UUID"
									aria-label="Reference target UUID"
									required
								/>
							{/if}
							<button
								class="btn btn-xs btn-primary"
								type="submit"
								disabled={$createLinkMutation.isPending}
							>
								Add reference
							</button>
						</form>
					{/if}
					{#if ($linksQuery.data ?? []).length === 0}
						<p class="text-xs text-base-content/50">No references yet.</p>
					{:else}
						<div class="flex flex-col gap-1.5">
							{#each $linksQuery.data ?? [] as link}
								<div
									class="flex items-center justify-between gap-2 rounded-md border border-[var(--rs-border)] px-2.5 py-1.5"
								>
									<div class="min-w-0">
										<div class="truncate text-xs font-medium">
											{link.target_type === 'file'
												? (($filesQuery.data ?? []).find((file) => file.id === link.target_id)
														?.name ?? 'Linked file')
												: (linkTargetTypes.find((type) => type.value === link.target_type)?.label ??
													link.target_type)}
										</div>
										<div class="truncate font-mono text-2xs text-base-content/45">
											{link.target_id}
										</div>
									</div>
									<button
										type="button"
										class="btn btn-xs btn-ghost text-error"
										onclick={() => deleteLinkMutation.mutate(link.id)}
										aria-label="Remove reference"
									>
										<Trash2 size={12} />
									</button>
								</div>
							{/each}
						</div>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
</div>

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
