<script lang="ts">
	import { goto } from '$app/navigation';
	import { browser } from '$app/environment';
	import { createMutation, createQuery } from '$lib/query-compat';
	import {
		mailApi,
		type ListMailAccountMessagesResponse,
		type ListMailMessagesResponse,
		type MailAccountMessage,
		type MailArchiveJob,
		type MailFolder,
		type MailImportJob,
		type MailMessage,
		type MailRemoteMessageBody,
		type MailSmtpSettings,
		type MailSortOrder,
		type SaveDraftRequest,
		type SendOutboundMailRequest
	} from '$lib/api/mail';
	import { apiClient } from '$lib/api/client';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import ModulePageSkeleton from '$lib/components/common/ModulePageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import MailComposeModal from './MailComposeModal.svelte';
	import MailMoveModal from './mail/MailMoveModal.svelte';
	import MailSaveModal from './mail/MailSaveModal.svelte';
	import { sanitizeEmailHtml } from '$lib/editor/adapter/security';
	import { mailBodyText, quoteMailBody, uniqueMailAddresses } from '$lib/mail/compose';
	import { toastStore } from '$lib/stores/toast';
	import {
		Archive,
		ArrowDownNarrowWide,
		ArrowDownWideNarrow,
		ArrowLeft,
		Check,
		ChevronDown,
		Download,
		FileText,
		Folder,
		Forward,
		Inbox,
		Loader2,
		Mail,
		MailOpen,
		MoreHorizontal,
		Paperclip,
		RefreshCw,
		Reply,
		ReplyAll,
		Search,
		Send,
		ShieldAlert,
		Star,
		Trash2
	} from 'lucide-svelte';
	import type { ModuleDefinition } from '$lib/modules/registry';

	let { module }: { module: ModuleDefinition } = $props();

	type MailboxView = 'remote' | 'drafts' | 'saved';
	type MobilePane = 'folders' | 'list' | 'viewer';
	type ComposeMode = 'new' | 'reply' | 'reply-all' | 'forward' | 'draft-edit';

	let selectedAccountId = $state<string | null>(null);
	let selectedFolder = $state<string | null>(null);
	let mailboxView = $state<MailboxView>('remote');
	let mobilePane = $state<MobilePane>('folders');
	let selectedUids = $state<number[]>([]);
	let selectedMessage = $state<MailAccountMessage | null>(null);
	let uidvalidity = $state<number | null>(null);
	let searchInput = $state('');
	let search = $state('');

	// One global sort preference for every mailbox and the Saved view,
	// persisted so it survives refresh and navigation away/back (issue #182).
	const MAIL_SORT_STORAGE_KEY = 'mail-sort-order';
	function readStoredSortOrder(): MailSortOrder {
		if (!browser) return 'date_desc';
		return localStorage.getItem(MAIL_SORT_STORAGE_KEY) === 'date_asc' ? 'date_asc' : 'date_desc';
	}
	let sortOrder = $state<MailSortOrder>(readStoredSortOrder());
	function toggleSortOrder() {
		sortOrder = sortOrder === 'date_desc' ? 'date_asc' : 'date_desc';
		if (browser) localStorage.setItem(MAIL_SORT_STORAGE_KEY, sortOrder);
	}
	let activityOpen = $state(false);
	let overflowOpen = $state(false);
	let moveOpen = $state(false);
	let saveOpen = $state(false);
	let actionPending = $state(false);
	let lastSyncedAt = $state<Date | null>(null);
	let mailboxInitialized = false;
	const refreshedImportJobs = new Set<string>();
	let uploadInput: HTMLInputElement | null = $state(null);

	let composeOpen = $state(false);
	let composeMode = $state<ComposeMode>('new');
	let composeDraftId = $state<string | null>(null);
	let composeTo = $state('');
	let composeCc = $state('');
	let composeBcc = $state('');
	let composeSubject = $state('');
	let composeBody = $state('');
	let composeAttachments = $state<string[]>([]);
	let composeInReplyTo = $state<string | null>(null);
	let composeReferences = $state<string[] | null>(null);
	let composeSaveError = $state('');
	let smtpSettings = $state<MailSmtpSettings | null>(null);

	const accountsQuery = createQuery({
		queryKey: ['mail-accounts'],
		queryFn: () => mailApi.listAccounts()
	});
	const foldersQuery = createQuery<MailFolder[]>({
		queryKey: ['mail-folders', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});
	const accountMessagesQuery = createQuery<ListMailAccountMessagesResponse>({
		queryKey: ['mail-account-messages', null, null, '', sortOrder],
		queryFn: () => Promise.resolve({ uidvalidity: null, next_cursor: null, messages: [] }),
		enabled: false
	});
	const remoteBodyQuery = createQuery<MailRemoteMessageBody>({
		queryKey: ['mail-remote-body', null],
		queryFn: () => Promise.reject(new Error('No message selected')),
		enabled: false
	});
	const draftsQuery = createQuery<MailMessage[]>({
		queryKey: ['mail-drafts', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});
	const importedMessagesQuery = createQuery<ListMailMessagesResponse>({
		queryKey: ['mail-messages', '', sortOrder],
		queryFn: () => mailApi.listMessagesPage('', null, null, sortOrder)
	});
	const archiveJobsQuery = createQuery<MailArchiveJob[]>({
		queryKey: ['mail-archive-jobs', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});
	const importJobsQuery = createQuery<MailImportJob[]>({
		queryKey: ['mail-import-jobs'],
		queryFn: () => mailApi.listImportJobs(),
		refetchInterval: 3000
	});

	let selectedAccount = $derived(
		($accountsQuery.data ?? []).find((account) => account.id === selectedAccountId) ?? null
	);
	let visibleMessages = $derived($accountMessagesQuery.data?.messages ?? []);
	let selectedActionUids = $derived(
		selectedUids.length ? selectedUids : selectedMessage ? [selectedMessage.uid] : []
	);
	// Remote images are blocked by default and only load after an explicit
	// per-message action; switching messages resets back to blocked.
	let remoteImagesAllowed = $state(false);
	let remoteImagesKey = $derived(
		selectedAccountId && selectedFolder && selectedMessage
			? `${selectedAccountId}:${selectedFolder}:${selectedMessage.uid}`
			: null
	);
	let lastRemoteImagesKey: string | null = null;
	$effect(() => {
		if (remoteImagesKey !== lastRemoteImagesKey) {
			lastRemoteImagesKey = remoteImagesKey;
			remoteImagesAllowed = false;
		}
	});

	function rewriteRemoteCidUrls(html: string, body: MailRemoteMessageBody): string {
		let rewritten = html;
		for (const attachment of body.attachments) {
			// Content-ID headers may be bracketed (`<id@host>`) while the
			// referencing URL is `cid:id@host`; match both forms.
			const contentId = attachment.content_id?.trim().replace(/^<+|>+$/g, '');
			if (!contentId) continue;
			const url = mailApi.remoteAttachmentUrl(
				selectedAccountId!,
				body.uid,
				attachment.index,
				selectedFolder!,
				uidvalidity
			);
			rewritten = rewritten.split(`cid:${contentId}`).join(url);
		}
		return rewritten;
	}

	let bodyRender = $derived.by(() => {
		const body = $remoteBodyQuery.data;
		if (!body?.html) return null;
		// cid: references are rewritten to attachment download URLs on our own
		// API before sanitization; exempt that base URL so blocked mode does
		// not strip them as "remote" images when the API URL is absolute.
		return sanitizeEmailHtml(rewriteRemoteCidUrls(body.html, body), {
			allowRemoteImages: remoteImagesAllowed,
			localUrlPrefixes: [apiClient.getBaseURL()]
		});
	});
	let safeBodyHtml = $derived(bodyRender?.html ?? null);
	let blockedRemoteImages = $derived(bodyRender?.blockedRemoteImages ?? 0);
	let syncing = $derived(
		$foldersQuery.isFetching || $accountMessagesQuery.isFetching || $draftsQuery.isFetching
	);

	$effect(() => {
		const accounts = $accountsQuery.data ?? [];
		if (!selectedAccountId && accounts.length) selectedAccountId = accounts[0].id;
		if (selectedAccountId && !accounts.some((account) => account.id === selectedAccountId)) {
			selectedAccountId = accounts[0]?.id ?? null;
		}
	});

	$effect(() => {
		foldersQuery.setOptions({
			queryKey: ['mail-folders', selectedAccountId],
			queryFn: () => mailApi.listFolders(selectedAccountId!),
			enabled: !!selectedAccountId
		});
		draftsQuery.setOptions({
			queryKey: ['mail-drafts', selectedAccountId],
			queryFn: () => mailApi.listDrafts(selectedAccountId!),
			enabled: !!selectedAccountId
		});
		archiveJobsQuery.setOptions({
			queryKey: ['mail-archive-jobs', selectedAccountId],
			queryFn: () => mailApi.listArchiveJobs(selectedAccountId!),
			enabled: !!selectedAccountId
		});
		if (selectedAccountId) {
			mailApi
				.getSmtpSettings(selectedAccountId)
				.then((settings) => (smtpSettings = settings))
				.catch(() => (smtpSettings = null));
		}
	});

	$effect(() => {
		const folders = $foldersQuery.data ?? [];
		if (!selectedFolder && folders.length) selectedFolder = folders[0].name;
		if (selectedFolder && !folders.some((folder) => folder.name === selectedFolder)) {
			selectedFolder = folders[0]?.name ?? null;
		}
	});

	$effect(() => {
		accountMessagesQuery.setOptions({
			queryKey: ['mail-account-messages', selectedAccountId, selectedFolder, search, sortOrder],
			queryFn: () =>
				mailApi.listAccountMessages(
					selectedAccountId!,
					selectedFolder!,
					100,
					null,
					search,
					sortOrder
				),
			enabled: mailboxView === 'remote' && !!selectedAccountId && !!selectedFolder
		});
		selectedUids = [];
		selectedMessage = null;
		if (mailboxInitialized) mobilePane = 'list';
		mailboxInitialized = true;
	});

	$effect(() => {
		uidvalidity = $accountMessagesQuery.data?.uidvalidity ?? null;
	});

	$effect(() => {
		for (const job of $importJobsQuery.data ?? []) {
			if (job.status === 'completed' && !refreshedImportJobs.has(job.id)) {
				refreshedImportJobs.add(job.id);
				void $accountMessagesQuery.refetch();
			}
		}
	});

	$effect(() => {
		importedMessagesQuery.setOptions({
			queryKey: ['mail-messages', search, sortOrder],
			queryFn: () => mailApi.listMessagesPage(search, null, null, sortOrder),
			enabled: mailboxView === 'saved'
		});
	});

	$effect(() => {
		remoteBodyQuery.setOptions({
			queryKey: [
				'mail-remote-body',
				selectedAccountId,
				selectedFolder,
				selectedMessage?.uid,
				uidvalidity
			],
			queryFn: () =>
				mailApi.getRemoteMessageBody(
					selectedAccountId!,
					selectedMessage!.uid,
					selectedFolder!,
					uidvalidity
				),
			enabled: !!selectedAccountId && !!selectedFolder && !!selectedMessage
		});
	});

	const sendMutation = createMutation({
		mutationFn: (input: SendOutboundMailRequest) =>
			mailApi.sendOutboundMail(selectedAccountId!, input),
		onSuccess: async () => {
			composeOpen = false;
			await $draftsQuery.refetch();
			toastStore.show('Message sent', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Send failed', 'error')
	});
	const saveDraftMutation = createMutation({
		mutationFn: ({ message, draftId }: { message: SaveDraftRequest; draftId: string | null }) =>
			draftId
				? mailApi.updateDraft(selectedAccountId!, draftId, message)
				: mailApi.saveDraft(selectedAccountId!, message),
		onSuccess: async (draft) => {
			composeDraftId = draft.id;
			composeSaveError = '';
			await $draftsQuery.refetch();
			toastStore.show('Draft saved', 'success');
		},
		onError: (error) => {
			composeSaveError = error instanceof Error ? error.message : 'Draft save failed';
			toastStore.show(composeSaveError, 'error');
		}
	});
	const sendDraftMutation = createMutation({
		mutationFn: async (message: SaveDraftRequest) => {
			const draft = await mailApi.updateDraft(selectedAccountId!, composeDraftId!, message);
			return mailApi.sendDraft(selectedAccountId!, draft.id);
		},
		onSuccess: async () => {
			composeOpen = false;
			await $draftsQuery.refetch();
			toastStore.show('Draft sent', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Draft send failed', 'error')
	});
	const discardDraftMutation = createMutation({
		mutationFn: (draftId: string) => mailApi.discardDraft(selectedAccountId!, draftId),
		onSuccess: async () => {
			composeOpen = false;
			await $draftsQuery.refetch();
			toastStore.show('Draft discarded', 'success');
		}
	});
	const uploadMutation = createMutation({
		mutationFn: (file: File) => mailApi.uploadMessage(file),
		onSuccess: async () => {
			await $importedMessagesQuery.refetch();
			toastStore.show('Mail imported', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Upload failed', 'error')
	});

	function resetCompose(mode: ComposeMode = 'new') {
		composeMode = mode;
		composeDraftId = null;
		composeTo = '';
		composeCc = '';
		composeBcc = '';
		composeSubject = '';
		composeBody = '';
		composeAttachments = [];
		composeInReplyTo = null;
		composeReferences = null;
		composeSaveError = '';
	}

	function openCompose() {
		resetCompose();
		composeOpen = true;
	}

	async function openDraft(message: MailMessage) {
		const draft = await mailApi.getDraft(selectedAccountId!, message.id);
		resetCompose('draft-edit');
		composeDraftId = message.id;
		composeTo = formatAddresses(draft.message.to_addresses);
		composeCc = formatAddresses(draft.message.cc_addresses);
		composeBcc = formatAddresses(draft.message.bcc_addresses);
		composeSubject = draft.message.subject ?? '';
		composeBody = draft.body;
		composeAttachments = draft.attachments;
		composeOpen = true;
	}

	function openReply(mode: 'reply' | 'reply-all' | 'forward') {
		const body = $remoteBodyQuery.data;
		if (!body) return;
		resetCompose(mode);
		const own = selectedAccount?.username ? [selectedAccount.username] : [];
		if (mode === 'reply') composeTo = body.from_address ?? '';
		if (mode === 'reply-all') {
			composeTo = uniqueMailAddresses(
				[body.from_address ?? '', ...body.to.map((address) => address.address)],
				own
			).join(', ');
			composeCc = uniqueMailAddresses(
				body.cc.map((address) => address.address),
				[...own, ...composeTo.split(',')]
			).join(', ');
		}
		composeSubject = `${mode === 'forward' ? 'Fwd:' : 'Re:'} ${(body.subject ?? '').replace(/^(re|fwd):\s*/i, '')}`;
		const text = mailBodyText({
			type: body.html ? 'html' : body.text ? 'text' : 'empty',
			content: body.html ?? body.text ?? ''
		});
		composeBody = `\n\n${mode === 'forward' ? '---------- Forwarded message ----------\n' : ''}${quoteMailBody(text)}`;
		if (mode !== 'forward' && body.message_id) {
			composeInReplyTo = body.message_id;
			composeReferences = [body.message_id];
		}
		composeOpen = true;
	}

	function formatAddresses(value: unknown): string {
		if (!Array.isArray(value)) return String(value ?? '');
		return value
			.map((item) => (typeof item === 'string' ? item : (item as { address?: string })?.address))
			.filter(Boolean)
			.join(', ');
	}

	function formatDate(value: string | null): string {
		if (!value) return '';
		return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(
			new Date(value)
		);
	}

	function formatBytes(value: number): string {
		if (value < 1024) return `${value} B`;
		if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
		return `${(value / 1024 / 1024).toFixed(1)} MB`;
	}

	// Duplicate attachment filenames are distinguished by their 1-based index
	// so two "report.pdf" chips never look identical.
	function hasDuplicateFilename(
		attachments: MailRemoteMessageBody['attachments'],
		index: number
	): boolean {
		const filename = attachments[index]?.filename;
		if (!filename) return false;
		return attachments.some(
			(other, otherIndex) => otherIndex !== index && other.filename === filename
		);
	}

	function selectMailbox(view: MailboxView, folder: string | null = selectedFolder) {
		mailboxView = view;
		selectedFolder = folder;
		selectedMessage = null;
		selectedUids = [];
		mobilePane = 'list';
	}

	async function selectRemoteMessage(message: MailAccountMessage) {
		selectedMessage = message;
		mobilePane = 'viewer';
		if (message.is_seen === false) {
			await mailApi.markMessageRead(selectedAccountId!, message.uid, selectedFolder!, uidvalidity);
			await $accountMessagesQuery.refetch();
		}
	}

	function toggleUid(uid: number) {
		selectedUids = selectedUids.includes(uid)
			? selectedUids.filter((selected) => selected !== uid)
			: [...selectedUids, uid];
	}

	async function runForSelection(
		action: (uid: number) => Promise<void>,
		success: string,
		verb: string,
		actionLabel: string
	) {
		if (!selectedActionUids.length || actionPending) return;
		actionPending = true;
		const uids = selectedActionUids;
		const failedUids: number[] = [];
		let firstError: unknown = null;
		try {
			for (const uid of uids) {
				try {
					await action(uid);
				} catch (error) {
					failedUids.push(uid);
					firstError ??= error;
				}
			}
			// Keep failed items selected so the user can retry them.
			if (selectedUids.length) {
				selectedUids = selectedUids.filter((uid) => failedUids.includes(uid));
			}
			if (
				selectedMessage &&
				uids.includes(selectedMessage.uid) &&
				!failedUids.includes(selectedMessage.uid)
			) {
				selectedMessage = null;
			}
			// Reconcile counts and unread state from the IMAP-authoritative state.
			await Promise.all([$foldersQuery.refetch(), $accountMessagesQuery.refetch()]);
			if (failedUids.length === 0) {
				toastStore.show(success, 'success');
			} else if (uids.length === 1) {
				// A single failed action deserves the server's real error, not a summary.
				const detail = firstError instanceof Error ? firstError.message : 'Unknown error';
				toastStore.show(`${actionLabel} failed: ${detail}`, 'error');
			} else {
				toastStore.show(
					`${verb} ${uids.length - failedUids.length} of ${uids.length} messages; ${failedUids.length} failed`,
					'error'
				);
			}
		} finally {
			actionPending = false;
		}
	}

	async function toggleStar(message: MailAccountMessage) {
		await runForSelection(
			(uid) =>
				(message.is_flagged ? mailApi.unstarMessage : mailApi.starMessage)(
					selectedAccountId!,
					uid,
					selectedFolder!,
					uidvalidity
				),
			message.is_flagged ? 'Star removed' : 'Message starred',
			message.is_flagged ? 'Unstarred' : 'Starred',
			message.is_flagged ? 'Unstar' : 'Star'
		);
	}

	async function confirmMove(destination: string) {
		await runForSelection(
			(uid) =>
				mailApi.moveMessage(selectedAccountId!, uid, selectedFolder!, destination, uidvalidity),
			selectedUids.length ? 'Messages moved' : 'Message moved',
			'Moved',
			'Move'
		);
		moveOpen = false;
	}

	async function archiveSelected() {
		const archiveFolder = ($foldersQuery.data ?? []).find((folder) => folder.role === 'archive');
		if (!archiveFolder) {
			toastStore.show('No archive folder found on this account', 'error');
			return;
		}
		await runForSelection(
			(uid) =>
				mailApi.archiveMessage(
					selectedAccountId!,
					uid,
					selectedFolder!,
					uidvalidity,
					archiveFolder.name
				),
			selectedUids.length ? 'Messages archived' : 'Message archived',
			'Archived',
			'Archive'
		);
	}

	async function confirmSave() {
		if (!selectedAccountId || !selectedFolder || !selectedActionUids.length) return;
		actionPending = true;
		try {
			await mailApi.createImportJob(selectedAccountId, {
				folder_name: selectedFolder,
				source_uidvalidity: uidvalidity,
				selected_uids: selectedActionUids
			});
			await $importJobsQuery.refetch();
			toastStore.show('Import job queued', 'success');
			saveOpen = false;
		} catch (error) {
			toastStore.show(error instanceof Error ? error.message : 'Import failed', 'error');
		} finally {
			actionPending = false;
		}
	}

	async function syncMailbox() {
		try {
			await Promise.all([
				$foldersQuery.refetch(),
				$accountMessagesQuery.refetch(),
				$draftsQuery.refetch(),
				$archiveJobsQuery.refetch(),
				$importJobsQuery.refetch()
			]);
			lastSyncedAt = new Date();
		} catch (error) {
			toastStore.show(error instanceof Error ? error.message : 'Synchronization failed', 'error');
		}
	}

	function submitSearch(event: SubmitEvent) {
		event.preventDefault();
		search = searchInput.trim();
	}

	function handleUploadChange(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (file) uploadMutation.mutate(file);
		input.value = '';
	}
</script>

<ModulePageShell title="Mail" subtitle={module.description}>
	<div slot="primaryAction">
		<button type="button" class="btn btn-primary btn-sm gap-2" onclick={openCompose}>
			<Send size={14} /> Compose
		</button>
	</div>

	{#if $accountsQuery.isLoading}
		<ModulePageSkeleton />
	{:else if $accountsQuery.isError}
		<ErrorState
			title="Failed to load accounts"
			message={$accountsQuery.error?.message ?? 'Unknown error'}
			onRetry={() => $accountsQuery.refetch()}
		/>
	{:else if ($accountsQuery.data ?? []).length === 0}
		<div
			class="mx-auto my-12 max-w-md rounded-xl border border-dashed border-base-300 p-10 text-center"
		>
			<Mail size={36} class="mx-auto mb-4 text-brand-500" />
			<h2 class="text-xl font-bold">No mail account configured</h2>
			<p class="mt-2 text-sm text-base-content/60">Configure an IMAP/SMTP account to use Mail.</p>
			<a href="/settings?tab=mail" class="btn btn-primary mt-6">Open Mail settings</a>
		</div>
	{:else}
		<section
			class="flex h-[calc(100vh-10rem)] min-h-[32rem] flex-col overflow-hidden rounded-xl border border-base-300 bg-base-100"
		>
			<header class="flex flex-wrap items-center gap-2 border-b border-base-300 p-2">
				<div class="min-w-48">
					<select
						class="select select-bordered select-sm w-full"
						aria-label="Mail account"
						bind:value={selectedAccountId}
						onchange={() => {
							selectedFolder = null;
							mailboxView = 'remote';
						}}
					>
						{#each $accountsQuery.data ?? [] as account}
							<option value={account.id}>{account.name}</option>
						{/each}
					</select>
					<p
						class="mt-0.5 truncate px-1 text-[11px] {selectedAccount?.last_error
							? 'text-error'
							: 'text-base-content/50'}"
					>
						{selectedAccount?.last_error
							? `Error: ${selectedAccount.last_error}`
							: syncing
								? 'Synchronizing…'
								: selectedAccount?.last_connected_at
									? `Connected ${formatDate(selectedAccount.last_connected_at)}`
									: 'Not synchronized yet'}
					</p>
				</div>
				<form class="relative min-w-44 flex-1" onsubmit={submitSearch}>
					<Search
						size={14}
						class="pointer-events-none absolute left-3 top-2.5 text-base-content/40"
					/>
					<input
						class="input input-bordered input-sm w-full pl-9"
						placeholder="Search mail"
						aria-label="Search mail"
						bind:value={searchInput}
					/>
				</form>
				<button
					type="button"
					class="btn btn-ghost btn-sm btn-square"
					aria-label={sortOrder === 'date_desc' ? 'Sort: newest first' : 'Sort: oldest first'}
					title={sortOrder === 'date_desc' ? 'Sort: newest first' : 'Sort: oldest first'}
					onclick={toggleSortOrder}
				>
					{#if sortOrder === 'date_desc'}<ArrowDownWideNarrow
							size={16}
						/>{:else}<ArrowDownNarrowWide size={16} />{/if}
				</button>
				<button
					type="button"
					class="btn btn-ghost btn-sm btn-square"
					aria-label="Synchronize mail"
					onclick={syncMailbox}
					disabled={syncing}
				>
					<RefreshCw size={16} class={syncing ? 'animate-spin' : ''} />
				</button>
				<div class="relative">
					<button
						type="button"
						class="btn btn-ghost btn-sm btn-square"
						aria-label="More mail actions"
						onclick={() => (overflowOpen = !overflowOpen)}
					>
						<MoreHorizontal size={18} />
					</button>
					{#if overflowOpen}
						<div
							class="absolute right-0 z-30 mt-1 w-56 rounded-lg border border-base-300 bg-base-100 p-1 shadow-xl"
						>
							<button
								type="button"
								class="btn btn-ghost btn-sm w-full justify-start"
								onclick={() => uploadInput?.click()}><Download size={14} /> Upload .eml</button
							>
							<a class="btn btn-ghost btn-sm w-full justify-start" href="/settings?tab=mail"
								><Folder size={14} /> Manage mail accounts</a
							>
							<button
								type="button"
								class="btn btn-ghost btn-sm w-full justify-start"
								onclick={() => {
									activityOpen = true;
									overflowOpen = false;
								}}><Archive size={14} /> Synchronization / Activity</button
							>
						</div>
					{/if}
				</div>
				<input
					bind:this={uploadInput}
					class="hidden"
					type="file"
					accept=".eml,message/rfc822"
					onchange={handleUploadChange}
				/>
				<span class="sr-only" aria-live="polite"
					>{syncing
						? 'Synchronizing mail'
						: lastSyncedAt
							? `Mail synchronized at ${lastSyncedAt.toLocaleTimeString()}`
							: ''}</span
				>
			</header>

			<div class="grid min-h-0 flex-1 lg:grid-cols-[220px_minmax(300px,360px)_minmax(0,1fr)]">
				<aside
					class="min-h-0 overflow-y-auto border-r border-base-300 {mobilePane === 'folders'
						? 'block'
						: 'hidden'} lg:block"
					aria-label="Mailboxes"
				>
					<div class="p-2">
						<p class="px-2 py-1 text-xs font-semibold uppercase tracking-wide text-base-content/45">
							Mailboxes
						</p>
						{#if $foldersQuery.isError}
							<div class="m-2 rounded-lg bg-error/10 p-3 text-sm text-error">
								<p>Folders could not be synchronized.</p>
								<button
									type="button"
									class="btn btn-xs btn-ghost mt-2"
									onclick={() => $foldersQuery.refetch()}>Retry</button
								>
							</div>
						{:else}
							{#each $foldersQuery.data ?? [] as folder}
								<button
									type="button"
									class="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-sm hover:bg-base-200 {mailboxView ===
										'remote' && selectedFolder === folder.name
										? 'bg-brand-500/10 font-semibold text-brand-700'
										: ''}"
									aria-current={mailboxView === 'remote' && selectedFolder === folder.name
										? 'page'
										: undefined}
									onclick={() => selectMailbox('remote', folder.name)}
								>
									{#if folder.role === 'archive'}<Archive
											size={15}
										/>{:else if folder.role === 'trash'}<Trash2 size={15} />{:else}<Inbox
											size={15}
										/>{/if}
									<span class="min-w-0 flex-1 truncate">{folder.display_name}</span>
									{#if folder.unseen}<span class="badge badge-primary badge-sm"
											>{folder.unseen}</span
										>{/if}
								</button>
							{/each}
						{/if}
						<div class="my-2 border-t border-base-300"></div>
						<button
							type="button"
							class="flex w-full items-center gap-2 rounded-md px-2 py-2 text-sm hover:bg-base-200 {mailboxView ===
							'drafts'
								? 'bg-brand-500/10 font-semibold text-brand-700'
								: ''}"
							aria-current={mailboxView === 'drafts' ? 'page' : undefined}
							onclick={() => selectMailbox('drafts')}
						>
							<FileText size={15} /><span class="flex-1 text-left">Drafts</span
							>{#if ($draftsQuery.data ?? []).length}<span class="badge badge-sm"
									>{($draftsQuery.data ?? []).length}</span
								>{/if}
						</button>
						<button
							type="button"
							class="flex w-full items-center gap-2 rounded-md px-2 py-2 text-sm hover:bg-base-200 {mailboxView ===
							'saved'
								? 'bg-brand-500/10 font-semibold text-brand-700'
								: ''}"
							aria-current={mailboxView === 'saved' ? 'page' : undefined}
							onclick={() => selectMailbox('saved')}
						>
							<Check size={15} /><span class="flex-1 text-left">Saved to RustShare</span>
						</button>
					</div>
				</aside>

				<section
					class="min-h-0 overflow-y-auto border-r border-base-300 {mobilePane === 'list'
						? 'block'
						: 'hidden'} lg:block"
					aria-label="Message list"
				>
					<div
						class="sticky top-0 z-10 flex items-center gap-2 border-b border-base-300 bg-base-100 p-2"
					>
						<button
							type="button"
							class="btn btn-ghost btn-xs btn-square lg:hidden"
							aria-label="Back to mailboxes"
							onclick={() => (mobilePane = 'folders')}><ArrowLeft size={15} /></button
						>
						<h2 class="min-w-0 flex-1 truncate text-sm font-semibold">
							{mailboxView === 'drafts'
								? 'Drafts'
								: mailboxView === 'saved'
									? 'Saved to RustShare'
									: (($foldersQuery.data ?? []).find((folder) => folder.name === selectedFolder)
											?.display_name ?? 'Messages')}
						</h2>
						{#if mailboxView === 'remote' && visibleMessages.length}
							<input
								type="checkbox"
								class="checkbox checkbox-sm"
								aria-label="Select all messages"
								checked={selectedUids.length === visibleMessages.length}
								onchange={() =>
									(selectedUids =
										selectedUids.length === visibleMessages.length
											? []
											: visibleMessages.map((message) => message.uid))}
							/>
						{/if}
					</div>
					{#if mailboxView === 'remote'}
						{#if $accountMessagesQuery.isLoading}<div class="flex justify-center p-8">
								<Loader2 class="animate-spin" />
							</div>
						{:else if $accountMessagesQuery.isError}<div class="p-6 text-center text-sm text-error">
								Messages could not be loaded.<button
									class="btn btn-xs btn-ghost ml-2"
									onclick={() => $accountMessagesQuery.refetch()}>Retry</button
								>
							</div>
						{:else if visibleMessages.length === 0}<p
								class="p-8 text-center text-sm text-base-content/50"
							>
								No messages in this folder.
							</p>
						{:else}
							<div class="divide-y divide-base-300">
								{#each visibleMessages as message (message.uid)}
									<div
										class="flex items-start gap-2 p-2 hover:bg-base-200/60 {selectedMessage?.uid ===
										message.uid
											? 'bg-brand-500/10'
											: ''}"
									>
										<input
											type="checkbox"
											class="checkbox checkbox-sm mt-2"
											aria-label="Select message {message.subject ?? '(No subject)'}"
											checked={selectedUids.includes(message.uid)}
											onchange={() => toggleUid(message.uid)}
										/>
										<button
											type="button"
											class="min-w-0 flex-1 rounded text-left focus:outline-none focus:ring-2 focus:ring-brand-500"
											onclick={() => selectRemoteMessage(message)}
										>
											<div class="flex items-center gap-2">
												<span
													class="min-w-0 flex-1 truncate text-sm {message.is_seen === false
														? 'font-bold'
														: 'font-medium'}"
													>{message.from_name || message.from_address || 'Unknown sender'}</span
												><time class="text-[11px] text-base-content/45"
													>{formatDate(message.sent_at)}</time
												>
											</div>
											<div class="mt-0.5 flex items-center gap-1">
												<span
													class="min-w-0 flex-1 truncate text-xs {message.is_seen === false
														? 'font-semibold'
														: 'text-base-content/65'}">{message.subject || '(No subject)'}</span
												>{#if message.is_flagged}<Star
														size={12}
														class="fill-warning text-warning"
													/>{/if}{#if message.imported_message_id}<Check
														size={12}
														class="text-success"
													/>{/if}
											</div>
										</button>
									</div>
								{/each}
							</div>
						{/if}
					{:else if mailboxView === 'drafts'}
						{#each $draftsQuery.data ?? [] as draft (draft.id)}<button
								type="button"
								class="block w-full border-b border-base-300 p-3 text-left hover:bg-base-200"
								onclick={() => openDraft(draft)}
								><p class="truncate text-sm font-semibold">{draft.subject || '(No subject)'}</p>
								<p class="truncate text-xs text-base-content/50">
									To: {formatAddresses(draft.to_addresses)}
								</p></button
							>{:else}<p class="p-8 text-center text-sm text-base-content/50">No drafts.</p>{/each}
					{:else}
						{#each $importedMessagesQuery.data?.messages ?? [] as message (message.id)}<button
								type="button"
								class="block w-full border-b border-base-300 p-3 text-left hover:bg-base-200"
								onclick={() => goto(`/modules/mail/messages/${message.id}`)}
								><p class="truncate text-sm font-semibold">{message.subject || '(No subject)'}</p>
								<p class="truncate text-xs text-base-content/50">
									{message.from_name || message.from_address || 'Unknown sender'}
								</p></button
							>{:else}<p class="p-8 text-center text-sm text-base-content/50">
								No saved mail.
							</p>{/each}
					{/if}
				</section>

				<article
					class="min-h-0 overflow-y-auto {mobilePane === 'viewer' ? 'block' : 'hidden'} lg:block"
					aria-label="Message viewer"
				>
					{#if !selectedMessage}
						<div
							class="flex h-full flex-col items-center justify-center p-8 text-center text-base-content/40"
						>
							<MailOpen size={42} />
							<p class="mt-3 text-sm">Select a message to read it.</p>
						</div>
					{:else}
						<div
							class="sticky top-0 z-10 flex flex-wrap items-center gap-1 border-b border-base-300 bg-base-100 p-2"
						>
							<button
								type="button"
								class="btn btn-ghost btn-xs btn-square lg:hidden"
								aria-label="Back to messages"
								onclick={() => (mobilePane = 'list')}><ArrowLeft size={15} /></button
							>
							<button
								type="button"
								class="btn btn-ghost btn-sm"
								onclick={() => openReply('reply')}
								disabled={!$remoteBodyQuery.data}><Reply size={14} /> Reply</button
							>
							<button
								type="button"
								class="btn btn-ghost btn-sm"
								onclick={() => openReply('reply-all')}
								disabled={!$remoteBodyQuery.data}><ReplyAll size={14} /> Reply all</button
							>
							<button
								type="button"
								class="btn btn-ghost btn-sm"
								onclick={() => openReply('forward')}
								disabled={!$remoteBodyQuery.data}><Forward size={14} /> Forward</button
							>
							<div class="flex-1"></div>
							<button
								type="button"
								class="btn btn-ghost btn-sm btn-square"
								aria-label={selectedMessage.is_flagged ? 'Remove star' : 'Star message'}
								title={selectedMessage.is_flagged ? 'Remove star' : 'Star message'}
								disabled={actionPending}
								onclick={() => toggleStar(selectedMessage!)}
								><Star
									size={15}
									class={selectedMessage.is_flagged ? 'fill-warning text-warning' : ''}
								/></button
							>
							<button
								type="button"
								class="btn btn-ghost btn-sm btn-square"
								aria-label="Save to RustShare"
								title="Save to RustShare"
								disabled={actionPending}
								onclick={() => (saveOpen = true)}><Check size={15} /></button
							>
							<a
								class="btn btn-ghost btn-sm btn-square"
								aria-label="Download .eml"
								download
								href={mailApi.remoteSourceUrl(
									selectedAccountId!,
									selectedMessage.uid,
									selectedFolder!,
									uidvalidity
								)}><Download size={15} /></a
							>
							<button
								type="button"
								class="btn btn-ghost btn-sm btn-square"
								aria-label="Move message"
								title="Move message"
								disabled={actionPending}
								onclick={() => (moveOpen = true)}><Folder size={15} /></button
							>
							<button
								type="button"
								class="btn btn-ghost btn-sm btn-square"
								aria-label="Archive message"
								title="Archive message"
								disabled={actionPending}
								onclick={archiveSelected}><Archive size={15} /></button
							>
							<button
								type="button"
								class="btn btn-ghost btn-sm btn-square text-error"
								aria-label="Delete message"
								title="Delete message"
								disabled={actionPending}
								onclick={() =>
									runForSelection(
										(uid) =>
											mailApi.deleteMessage(selectedAccountId!, uid, selectedFolder!, uidvalidity),
										'Message deleted',
										'Deleted',
										'Delete'
									)}><Trash2 size={15} /></button
							>
						</div>
						{#if $remoteBodyQuery.isLoading}<div class="flex justify-center p-10">
								<Loader2 class="animate-spin" />
							</div>
						{:else if $remoteBodyQuery.isError}<div class="p-8 text-center text-sm text-error">
								Message body could not be loaded.<button
									class="btn btn-xs btn-ghost ml-2"
									onclick={() => $remoteBodyQuery.refetch()}>Retry</button
								>
							</div>
						{:else if $remoteBodyQuery.data}
							<div class="p-5">
								<h2 class="text-xl font-bold">{$remoteBodyQuery.data.subject || '(No subject)'}</h2>
								<div class="mt-3 flex flex-wrap justify-between gap-2 text-sm">
									<div>
										<p class="font-semibold">
											{$remoteBodyQuery.data.from_name ||
												$remoteBodyQuery.data.from_address ||
												'Unknown sender'}
										</p>
										<p class="text-xs text-base-content/50">
											To: {$remoteBodyQuery.data.to.map((address) => address.address).join(', ')}
										</p>
									</div>
									<time class="text-xs text-base-content/50"
										>{formatDate($remoteBodyQuery.data.date)}</time
									>
								</div>
								{#if $remoteBodyQuery.data.html}
									{#if blockedRemoteImages > 0 && !remoteImagesAllowed}
										<div
											class="mt-4 flex items-center gap-2 rounded-lg border border-base-300 bg-base-200/60 px-3 py-2 text-sm"
										>
											<ShieldAlert size={16} class="shrink-0 text-warning" />
											<span class="flex-1">Images were blocked to protect your privacy.</span>
											<button
												class="btn btn-outline btn-xs"
												onclick={() => (remoteImagesAllowed = true)}>Load remote images</button
											>
										</div>
									{:else if remoteImagesAllowed}
										<div
											class="mt-4 flex items-center gap-2 rounded-lg border border-base-300 bg-base-200/40 px-3 py-2 text-xs text-base-content/60"
										>
											<ShieldAlert size={14} class="shrink-0" />
											<span class="flex-1">Remote images loaded for this message.</span>
											<button
												class="btn btn-ghost btn-xs"
												onclick={() => (remoteImagesAllowed = false)}>Block images</button
											>
										</div>
									{/if}
								{/if}
								<div class="prose mt-6 max-w-none text-sm">
									{#if safeBodyHtml}{@html safeBodyHtml}{:else}<pre
											class="whitespace-pre-wrap font-sans">{$remoteBodyQuery.data.text ||
												'This message has no readable body.'}</pre>{/if}
								</div>
								{#if $remoteBodyQuery.data.attachments.length}<div
										class="mt-8 border-t border-base-300 pt-4"
									>
										<h3 class="mb-2 text-sm font-semibold">Attachments</h3>
										<div class="flex flex-wrap gap-2">
											{#each $remoteBodyQuery.data.attachments as attachment, attachmentIndex}<a
													class="btn btn-outline btn-sm"
													href={mailApi.remoteAttachmentUrl(
														selectedAccountId!,
														selectedMessage.uid,
														attachment.index,
														selectedFolder!,
														uidvalidity
													)}
													><Paperclip size={13} />{attachment.filename ||
														`Attachment ${attachment.index + 1}`}{#if hasDuplicateFilename($remoteBodyQuery.data.attachments, attachmentIndex)}<span
															class="badge badge-ghost badge-sm">#{attachment.index + 1}</span
														>{/if}
													<span class="text-base-content/45"
														>{formatBytes(attachment.size_bytes)}</span
													></a
												>{/each}
										</div>
									</div>{/if}
							</div>
						{/if}
					{/if}
				</article>
			</div>

			{#if selectedUids.length}
				<div
					class="absolute bottom-4 left-1/2 z-20 flex -translate-x-1/2 items-center gap-1 rounded-xl border border-base-300 bg-base-100 p-2 shadow-xl"
				>
					<span class="px-2 text-sm font-semibold">{selectedUids.length} selected</span>
					<button
						class="btn btn-ghost btn-sm"
						disabled={actionPending}
						onclick={() =>
							runForSelection(
								(uid) =>
									mailApi.markMessageRead(selectedAccountId!, uid, selectedFolder!, uidvalidity),
								'Marked read',
								'Marked read',
								'Mark read'
							)}>Read</button
					>
					<button
						class="btn btn-ghost btn-sm"
						disabled={actionPending}
						onclick={() =>
							runForSelection(
								(uid) =>
									mailApi.markMessageUnread(selectedAccountId!, uid, selectedFolder!, uidvalidity),
								'Marked unread',
								'Marked unread',
								'Mark unread'
							)}>Unread</button
					>
					<button
						class="btn btn-ghost btn-sm"
						disabled={actionPending}
						onclick={() =>
							runForSelection(
								(uid) => mailApi.starMessage(selectedAccountId!, uid, selectedFolder!, uidvalidity),
								'Messages starred',
								'Starred',
								'Star'
							)}>Star</button
					>
					<button
						class="btn btn-ghost btn-sm"
						disabled={actionPending}
						onclick={() => (saveOpen = true)}>Save</button
					>
					<button
						class="btn btn-ghost btn-sm"
						disabled={actionPending}
						onclick={() => (moveOpen = true)}>Move</button
					>
					<button class="btn btn-ghost btn-sm" disabled={actionPending} onclick={archiveSelected}
						>Archive</button
					>
					<button
						class="btn btn-ghost btn-sm text-error"
						disabled={actionPending}
						onclick={() =>
							runForSelection(
								(uid) =>
									mailApi.deleteMessage(selectedAccountId!, uid, selectedFolder!, uidvalidity),
								'Messages deleted',
								'Deleted',
								'Delete'
							)}>Delete</button
					>
				</div>
			{/if}
		</section>
	{/if}
</ModulePageShell>

{#if activityOpen}
	<div class="modal modal-open">
		<div class="modal-box max-w-2xl">
			<h2 class="text-lg font-bold">Synchronization activity</h2>
			<div class="mt-4 grid gap-4 sm:grid-cols-2">
				<section>
					<h3 class="mb-2 text-sm font-semibold">Imports</h3>
					{#each ($importJobsQuery.data ?? []).slice(0, 8) as job}<div
							class="mb-2 rounded border border-base-300 p-2 text-xs"
						>
							<p class="font-medium">{job.folder_name} · {job.status}</p>
							<p>{job.processed_messages}/{job.total_messages} processed</p>
							{#if job.last_error}<p class="text-error">{job.last_error}</p>{/if}
						</div>{:else}<p class="text-sm text-base-content/50">No recent imports.</p>{/each}
				</section>
				<section>
					<h3 class="mb-2 text-sm font-semibold">Archive jobs</h3>
					{#each ($archiveJobsQuery.data ?? []).slice(0, 8) as job}<div
							class="mb-2 rounded border border-base-300 p-2 text-xs"
						>
							<p class="font-medium">{job.status}</p>
							<p>{job.processed_messages}/{job.total_messages} processed</p>
						</div>{:else}<p class="text-sm text-base-content/50">No recent archive jobs.</p>{/each}
				</section>
			</div>
			<div class="modal-action">
				<button class="btn" onclick={() => (activityOpen = false)}>Close</button>
			</div>
		</div>
		<button
			class="modal-backdrop"
			aria-label="Close activity"
			onclick={() => (activityOpen = false)}>Close</button
		>
	</div>
{/if}

<MailMoveModal
	open={moveOpen}
	folders={$foldersQuery.data ?? []}
	currentFolder={selectedFolder}
	isLoading={actionPending}
	onClose={() => (moveOpen = false)}
	onMove={confirmMove}
/>
<MailSaveModal
	open={saveOpen}
	count={selectedActionUids.length || 1}
	alreadySaved={selectedActionUids.length === 1 && !!selectedMessage?.imported_message_id}
	importedMessageId={selectedMessage?.imported_message_id}
	isLoading={actionPending}
	onClose={() => (saveOpen = false)}
	onConfirm={confirmSave}
/>
<MailComposeModal
	open={composeOpen}
	initialTo={composeTo}
	initialCc={composeCc}
	initialBcc={composeBcc}
	initialSubject={composeSubject}
	initialBody={composeBody}
	initialAttachments={composeAttachments}
	inReplyTo={composeInReplyTo}
	references={composeReferences}
	mode={composeMode}
	draftId={composeDraftId}
	sending={$sendMutation.isPending || $sendDraftMutation.isPending}
	saving={$saveDraftMutation.isPending}
	discarding={$discardDraftMutation.isPending}
	hasSmtp={!!smtpSettings?.is_enabled}
	saveError={composeSaveError}
	onClose={() => (composeOpen = false)}
	onSend={(message) =>
		composeDraftId ? sendDraftMutation.mutate(message) : sendMutation.mutate(message)}
	onSave={async (message, draftId) => {
		await saveDraftMutation.mutateAsync({ message, draftId });
	}}
	onDiscard={(draftId) => discardDraftMutation.mutate(draftId)}
/>
