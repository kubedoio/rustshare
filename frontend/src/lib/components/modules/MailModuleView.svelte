<script lang="ts">
	import { createMutation, createQuery } from '$lib/query-compat';
	import {
		mailApi,
		type MailAccountMessage,
		type MailArchiveJob,
		type MailImportJob,
		type MailFolder,
		type ListMailAccountMessagesResponse,
		type ListMailMessagesResponse,
		type MailMessage,
		type MailSmtpSettings,
		type SaveDraftRequest,
		type SendOutboundMailRequest
	} from '$lib/api/mail';
	import ModulePageSkeleton from '$lib/components/common/ModulePageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import MailComposeModal from '$lib/components/modules/MailComposeModal.svelte';
	import MailToolbar from '$lib/components/modules/mail/MailToolbar.svelte';
	import MailFolderPane from '$lib/components/modules/mail/MailFolderPane.svelte';
	import MailMessageList from '$lib/components/modules/mail/MailMessageList.svelte';
	import MailMessageViewer, {
		type ComposeRequest
	} from '$lib/components/modules/mail/MailMessageViewer.svelte';
	import MailActivityDrawer from '$lib/components/modules/mail/MailActivityDrawer.svelte';
	import {
		findFolderByRole,
		formatMailAddresses,
		type FolderSelection,
		type MailListItem,
		type ViewerTarget
	} from '$lib/components/modules/mail/mail-types';
	import { toastStore } from '$lib/stores/toast';
	import { Mail } from 'lucide-svelte';
	import type { ModuleDefinition } from '$lib/modules/registry';

	let { module }: { module: ModuleDefinition } = $props();

	// ---------------------------------------------------------------- state
	let selectedAccountId = $state<string | null>(null);
	let selection = $state<FolderSelection>({ kind: 'imported' });
	let viewerTarget = $state<ViewerTarget>(null);
	let mobileView = $state<'folders' | 'list' | 'viewer'>('list');
	let search = $state('');
	let checkedUids = $state<number[]>([]);
	let uidvalidity = $state<number | null>(null);
	let activityOpen = $state(false);

	let mailboxExtraMessages = $state<MailAccountMessage[]>([]);
	let mailboxNextCursor = $state<number | null>(null);
	let importedExtraMessages = $state<MailMessage[]>([]);
	let importedNextCursorAt = $state<string | null>(null);
	let importedNextCursorId = $state<string | null>(null);
	let loadingMore = $state(false);

	let uploadInput: HTMLInputElement | null = $state(null);
	let composeOpen = $state(false);
	let composeDraftId = $state<string | null>(null);
	let composeMode = $state<'new' | 'reply' | 'reply-all' | 'forward' | 'draft-edit'>('new');
	let composeTo = $state('');
	let composeCc = $state('');
	let composeBcc = $state('');
	let composeSubject = $state('');
	let composeBody = $state('');
	let composeAttachments = $state<string[]>([]);
	let composeReplyTo = $state<string | null>(null);
	let composeSaveError = $state('');

	// ---------------------------------------------------------------- queries
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
		queryKey: ['mail-account-messages', null, null],
		queryFn: () => Promise.resolve({ uidvalidity: null, next_cursor: null, messages: [] }),
		enabled: false
	});

	const draftsQuery = createQuery<MailMessage[]>({
		queryKey: ['mail-drafts', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});

	const importedMessagesQuery = createQuery<ListMailMessagesResponse>({
		queryKey: ['mail-messages', ''],
		queryFn: () => mailApi.listMessagesPage()
	});

	const importJobsQuery = createQuery<MailImportJob[]>({
		queryKey: ['mail-import-jobs'],
		queryFn: () => mailApi.listImportJobs(),
		refetchInterval: 3000
	});

	const archiveJobsQuery = createQuery<MailArchiveJob[]>({
		queryKey: ['mail-archive-jobs', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});

	// ------------------------------------------------------------- selection
	$effect(() => {
		const accounts = $accountsQuery.data ?? [];
		if (!selectedAccountId && accounts.length > 0) selectedAccountId = accounts[0].id;
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
			enabled: !!selectedAccountId,
			refetchInterval: 5000
		});
	});

	// Fall back to the local Imported folder when the selected IMAP folder
	// disappears (account switch or folder refresh).
	$effect(() => {
		if (selection.kind !== 'imap') return;
		const folders = $foldersQuery.data;
		if (!folders || folders.length === 0) return;
		if (!folders.some((folder) => folder.name === (selection as { name: string }).name)) {
			selection = { kind: 'imported' };
		}
	});

	$effect(() => {
		checkedUids = [];
		mailboxExtraMessages = [];
		mailboxNextCursor = null;
		const folderName = selection.kind === 'imap' ? selection.name : null;
		accountMessagesQuery.setOptions({
			queryKey: ['mail-account-messages', selectedAccountId, folderName, search],
			queryFn: () =>
				mailApi.listAccountMessages(selectedAccountId!, folderName!, 100, null, search),
			enabled: !!selectedAccountId && !!folderName
		});
	});

	$effect(() => {
		const data = $accountMessagesQuery.data;
		uidvalidity = data?.uidvalidity ?? null;
		mailboxNextCursor = data?.next_cursor ?? null;
	});

	$effect(() => {
		importedExtraMessages = [];
		importedMessagesQuery.setOptions({
			queryKey: ['mail-messages', search],
			queryFn: () => mailApi.listMessagesPage(search)
		});
	});

	$effect(() => {
		importedNextCursorAt = $importedMessagesQuery.data?.next_cursor_at ?? null;
		importedNextCursorId = $importedMessagesQuery.data?.next_cursor_id ?? null;
	});

	// ------------------------------------------------------------- derived
	let selectedAccount = $derived(
		($accountsQuery.data ?? []).find((account) => account.id === selectedAccountId) ?? null
	);

	let folders = $derived($foldersQuery.data ?? []);
	let archiveFolderName = $derived(
		findFolderByRole(folders, 'archive', ['archive', 'all mail', '[gmail]/all mail'])
	);
	let trashFolderName = $derived(
		findFolderByRole(folders, 'trash', ['trash', 'deleted items', '[gmail]/trash'])
	);

	let listItems = $derived.by((): MailListItem[] => {
		if (selection.kind === 'imap') {
			return [...($accountMessagesQuery.data?.messages ?? []), ...mailboxExtraMessages].map(
				(message) => ({ kind: 'imap' as const, uid: message.uid, message })
			);
		}
		if (selection.kind === 'drafts') {
			const term = search.trim().toLowerCase();
			return ($draftsQuery.data ?? [])
				.filter(
					(message) =>
						!term ||
						(message.subject ?? '').toLowerCase().includes(term) ||
						formatMailAddresses(message.to_addresses).toLowerCase().includes(term)
				)
				.map((message) => ({ kind: 'stored' as const, id: message.id, message }));
		}
		const seen = new Map<string, MailMessage>();
		for (const message of [
			...($importedMessagesQuery.data?.messages ?? []),
			...importedExtraMessages
		]) {
			if (!seen.has(message.id)) seen.set(message.id, message);
		}
		return [...seen.values()].map((message) => ({
			kind: 'stored' as const,
			id: message.id,
			message
		}));
	});

	let listTitle = $derived.by(() => {
		if (selection.kind === 'imap') {
			const folder = folders.find((f) => f.name === (selection as { name: string }).name);
			return folder?.display_name ?? (selection as { name: string }).name;
		}
		return selection.kind === 'drafts' ? 'Drafts' : 'Imported';
	});

	let listLoading = $derived(
		selection.kind === 'imap'
			? $accountMessagesQuery.isLoading
			: selection.kind === 'drafts'
				? $draftsQuery.isLoading
				: $importedMessagesQuery.isLoading
	);
	let listRefreshing = $derived(
		selection.kind === 'imap'
			? $accountMessagesQuery.isFetching && !$accountMessagesQuery.isLoading
			: selection.kind === 'drafts'
				? $draftsQuery.isFetching && !$draftsQuery.isLoading
				: $importedMessagesQuery.isFetching && !$importedMessagesQuery.isLoading
	);
	let listError = $derived(
		selection.kind === 'imap'
			? $accountMessagesQuery.isError
				? ($accountMessagesQuery.error?.message ?? 'Unknown error')
				: null
			: selection.kind === 'drafts'
				? $draftsQuery.isError
					? ($draftsQuery.error?.message ?? 'Unknown error')
					: null
				: $importedMessagesQuery.isError
					? ($importedMessagesQuery.error?.message ?? 'Unknown error')
					: null
	);
	let listHasMore = $derived(
		selection.kind === 'imap'
			? !!mailboxNextCursor
			: selection.kind === 'imported'
				? !!(importedNextCursorAt && importedNextCursorId)
				: false
	);

	let selectedKey = $derived.by(() => {
		if (!viewerTarget) return null;
		return viewerTarget.kind === 'imap'
			? `imap:${viewerTarget.message.uid}`
			: `stored:${viewerTarget.id}`;
	});

	let activeJobLabel = $derived.by(() => {
		const importJob = ($importJobsQuery.data ?? []).find((job) =>
			['pending', 'running'].includes(job.status)
		);
		if (importJob) return `Importing ${importJob.processed_messages}/${importJob.total_messages}`;
		const archiveJob = ($archiveJobsQuery.data ?? []).find((job) =>
			['pending', 'running'].includes(job.status)
		);
		if (archiveJob)
			return `Archiving ${archiveJob.processed_messages}/${archiveJob.total_messages}`;
		return null;
	});

	// ------------------------------------------------------------- mutations
	const importMutation = createMutation({
		mutationFn: (uids: number[]) =>
			mailApi.createImportJob(selectedAccountId!, {
				folder_name: selection.kind === 'imap' ? selection.name : '',
				source_uidvalidity: uidvalidity,
				selected_uids: uids
			}),
		onSuccess: async () => {
			checkedUids = [];
			toastStore.show('Import job queued', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Import failed', 'error')
	});

	const uploadMutation = createMutation({
		mutationFn: (file: File) => mailApi.uploadMessage(file),
		onSuccess: async (message) => {
			await $importedMessagesQuery.refetch();
			toastStore.show('Mail imported', 'success');
			selection = { kind: 'imported' };
			viewerTarget = { kind: 'stored', id: message.id };
			mobileView = 'viewer';
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Upload failed', 'error')
	});

	const sendMutation = createMutation({
		mutationFn: (input: SendOutboundMailRequest) => {
			if (!selectedAccountId) throw new Error('Select a mail account to compose/send.');
			if (composeMode === 'reply') return mailApi.replyMail(selectedAccountId, input);
			if (composeMode === 'reply-all') return mailApi.replyAllMail(selectedAccountId, input);
			if (composeMode === 'forward') return mailApi.forwardMail(selectedAccountId, input);
			return mailApi.sendOutboundMail(selectedAccountId, input);
		},
		onSuccess: async (result) => {
			composeOpen = false;
			await $importedMessagesQuery.refetch();
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
		mutationFn: ({ message, draftId }: { message: SaveDraftRequest; draftId: string | null }) => {
			if (!selectedAccountId) throw new Error('Select a mail account to save a draft.');
			return draftId
				? mailApi.updateDraft(selectedAccountId, draftId, message)
				: mailApi.saveDraft(selectedAccountId, message);
		},
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
		mutationFn: (draftId: string) => {
			if (!selectedAccountId) throw new Error('Select a mail account to send a draft.');
			return mailApi.sendDraft(selectedAccountId, draftId);
		},
		onSuccess: async (result) => {
			composeOpen = false;
			composeDraftId = null;
			await $draftsQuery.refetch();
			await $importedMessagesQuery.refetch();
			toastStore.show(
				!result.stored
					? 'Draft sent, but the RustShare copy could not be saved'
					: result.append_failed
						? 'Draft sent, but not saved to the Sent folder'
						: 'Draft sent',
				!result.stored || result.append_failed ? 'info' : 'success'
			);
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Draft send failed', 'error')
	});

	const discardDraftMutation = createMutation({
		mutationFn: (draftId: string) => {
			if (!selectedAccountId) throw new Error('Select a mail account to discard a draft.');
			return mailApi.discardDraft(selectedAccountId, draftId);
		},
		onSuccess: async () => {
			composeOpen = false;
			composeDraftId = null;
			await $draftsQuery.refetch();
			toastStore.show('Draft discarded', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Draft discard failed', 'error')
	});

	// ------------------------------------------------------------- SMTP
	let smtpSettings = $state<MailSmtpSettings | null>(null);
	$effect(() => {
		smtpSettings = null;
		if (!selectedAccountId) return;
		mailApi
			.getSmtpSettings(selectedAccountId)
			.then((settings) => (smtpSettings = settings))
			.catch(() => (smtpSettings = null));
	});

	// ------------------------------------------------------------- handlers
	function handleSelectAccount(accountId: string) {
		selectedAccountId = accountId;
		viewerTarget = null;
		checkedUids = [];
	}

	function handleSelectFolder(next: FolderSelection) {
		selection = next;
		viewerTarget = null;
		checkedUids = [];
		mobileView = 'list';
	}

	function handleOpenItem(item: MailListItem) {
		if (item.kind === 'imap') {
			viewerTarget = { kind: 'imap', message: item.message };
			mobileView = 'viewer';
			return;
		}
		if (selection.kind === 'drafts') {
			openDraft(item.message);
			return;
		}
		viewerTarget = { kind: 'stored', id: item.id };
		mobileView = 'viewer';
	}

	function handleViewerBack() {
		viewerTarget = null;
		mobileView = 'list';
	}

	async function refreshCurrent() {
		if (selection.kind === 'imap') {
			mailboxExtraMessages = [];
			mailboxNextCursor = null;
			await Promise.all([$foldersQuery.refetch(), $accountMessagesQuery.refetch()]);
		} else if (selection.kind === 'drafts') {
			await $draftsQuery.refetch();
		} else {
			importedExtraMessages = [];
			await $importedMessagesQuery.refetch();
		}
	}

	async function loadMore() {
		loadingMore = true;
		try {
			if (selection.kind === 'imap' && mailboxNextCursor && selectedAccountId) {
				const page = await mailApi.listAccountMessages(
					selectedAccountId,
					selection.name,
					100,
					mailboxNextCursor,
					search
				);
				const known = new Set(
					listItems.filter((i) => i.kind === 'imap').map((i) => (i.kind === 'imap' ? i.uid : 0))
				);
				mailboxExtraMessages = [
					...mailboxExtraMessages,
					...page.messages.filter((message) => !known.has(message.uid))
				];
				mailboxNextCursor = page.next_cursor;
			} else if (selection.kind === 'imported' && importedNextCursorAt && importedNextCursorId) {
				const page = await mailApi.listMessagesPage(
					search,
					importedNextCursorAt,
					importedNextCursorId
				);
				const seen = new Set(listItems.map((item) => (item.kind === 'stored' ? item.id : '')));
				importedExtraMessages = [
					...importedExtraMessages,
					...page.messages.filter((message) => !seen.has(message.id))
				];
				importedNextCursorAt = page.next_cursor_at;
				importedNextCursorId = page.next_cursor_id;
			}
		} catch (error) {
			toastStore.show(error instanceof Error ? error.message : 'Failed to load more', 'error');
		} finally {
			loadingMore = false;
		}
	}

	async function runImapAction(
		action: 'read' | 'unread' | 'archive' | 'trash' | 'delete',
		uid: number,
		options: { silent?: boolean } = {}
	) {
		if (!selectedAccountId || selection.kind !== 'imap') return;
		const folder = selection.name;
		if (
			!options.silent &&
			action === 'delete' &&
			!confirm('Delete this message from the mailbox permanently?')
		)
			return;
		try {
			if (action === 'read')
				await mailApi.markMessageRead(selectedAccountId, uid, folder, uidvalidity);
			else if (action === 'unread')
				await mailApi.markMessageUnread(selectedAccountId, uid, folder, uidvalidity);
			else if (action === 'archive')
				await mailApi.archiveMessage(
					selectedAccountId,
					uid,
					folder,
					uidvalidity,
					archiveFolderName
				);
			else if (action === 'trash')
				await mailApi.trashMessage(selectedAccountId, uid, folder, uidvalidity, trashFolderName);
			else await mailApi.deleteMessage(selectedAccountId, uid, folder, uidvalidity);
			if (
				viewerTarget?.kind === 'imap' &&
				viewerTarget.message.uid === uid &&
				action !== 'read' &&
				action !== 'unread'
			) {
				viewerTarget = null;
				mobileView = 'list';
			}
			if (options.silent) return;
			await refreshCurrent();
			toastStore.show(
				action === 'read'
					? 'Marked read'
					: action === 'unread'
						? 'Marked unread'
						: action === 'archive'
							? 'Archived message'
							: action === 'trash'
								? 'Moved to trash'
								: 'Deleted message',
				'success'
			);
		} catch (error) {
			toastStore.show(error instanceof Error ? error.message : 'Action failed', 'error');
		}
	}

	async function runBulkAction(action: 'archive' | 'trash' | 'delete') {
		if (checkedUids.length === 0) return;
		if (action === 'delete' && !confirm(`Delete ${checkedUids.length} message(s) permanently?`))
			return;
		const count = checkedUids.length;
		for (const uid of checkedUids) {
			// Sequential to preserve server-side UID validity handling

			await runImapAction(action, uid, { silent: true });
		}
		checkedUids = [];
		await refreshCurrent();
		toastStore.show(
			action === 'archive'
				? `Archived ${count} message(s)`
				: action === 'trash'
					? `Moved ${count} message(s) to trash`
					: `Deleted ${count} message(s)`,
			'success'
		);
	}

	function toggleUid(uid: number) {
		checkedUids = checkedUids.includes(uid)
			? checkedUids.filter((checked) => checked !== uid)
			: [...checkedUids, uid];
	}

	function openCompose() {
		composeMode = 'new';
		composeDraftId = null;
		composeTo = '';
		composeCc = '';
		composeBcc = '';
		composeSubject = '';
		composeBody = '';
		composeAttachments = [];
		composeReplyTo = null;
		composeSaveError = '';
		composeOpen = true;
	}

	function openComposeRequest(request: ComposeRequest) {
		composeMode = request.mode;
		composeDraftId = null;
		composeTo = request.to;
		composeCc = request.cc;
		composeBcc = request.bcc;
		composeSubject = request.subject;
		composeBody = request.body;
		composeAttachments = request.attachments;
		composeReplyTo = request.inReplyTo;
		composeSaveError = '';
		composeOpen = true;
	}

	async function openDraft(message: MailMessage) {
		if (!selectedAccountId) return;
		try {
			const draft = await mailApi.getDraft(selectedAccountId, message.id);
			composeMode = 'draft-edit';
			composeDraftId = message.id;
			composeTo = formatMailAddresses(draft.message.to_addresses);
			composeCc = formatMailAddresses(draft.message.cc_addresses);
			composeBcc = formatMailAddresses(draft.message.bcc_addresses);
			composeSubject = draft.message.subject ?? '';
			composeBody = draft.body;
			composeAttachments = draft.attachments;
			composeReplyTo = draft.message.in_reply_to ?? null;
			composeSaveError = '';
			composeOpen = true;
		} catch (error) {
			toastStore.show(error instanceof Error ? error.message : 'Draft no longer exists', 'error');
			await $draftsQuery.refetch();
		}
	}

	async function sendCompose(message: SaveDraftRequest) {
		if (!composeDraftId) {
			await sendMutation.mutate(message);
			return;
		}
		await saveDraftMutation.mutate({ message, draftId: composeDraftId });
		await sendDraftMutation.mutate(composeDraftId);
	}

	function handleUploadChange(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (file) uploadMutation.mutate(file);
		input.value = '';
	}
</script>

<input
	bind:this={uploadInput}
	class="hidden"
	type="file"
	accept=".eml,message/rfc822"
	aria-label="Upload .eml file"
	onchange={handleUploadChange}
/>

{#if $accountsQuery.isLoading}
	<ModulePageSkeleton />
{:else if $accountsQuery.isError}
	<ErrorState
		title="Failed to load accounts"
		message={$accountsQuery.error?.message || 'Unknown error'}
		onRetry={() => $accountsQuery.refetch()}
	/>
{:else if ($accountsQuery.data ?? []).length === 0}
	<div class="mx-auto my-12 max-w-md text-center">
		<div
			class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-brand-500/10 text-brand-600"
		>
			<Mail size={32} />
		</div>
		<h2 class="text-xl font-bold text-base-content">No mail account configured</h2>
		<p class="mt-2 mb-6 text-sm text-base-content/60">
			Configure an IMAP/SMTP account in Settings to use Mail. You can still upload .eml files.
		</p>
		<div class="flex justify-center gap-2">
			<button type="button" class="btn btn-outline" onclick={() => uploadInput?.click()}>
				Upload .eml
			</button>
			<a href="/settings?tab=mail" class="btn btn-primary">Open Mail settings</a>
		</div>
	</div>
{:else}
	<div
		class="flex h-full min-h-0 flex-col overflow-hidden rounded-lg border border-[var(--rs-border)] bg-[var(--rs-surface-raised)]"
	>
		<MailToolbar
			accounts={$accountsQuery.data ?? []}
			{selectedAccountId}
			searchValue={search}
			refreshing={listRefreshing}
			{activeJobLabel}
			onSelectAccount={handleSelectAccount}
			onSearch={(value) => (search = value)}
			onClearSearch={() => (search = '')}
			onRefresh={refreshCurrent}
			onCompose={openCompose}
			onUploadEml={() => uploadInput?.click()}
			onOpenActivity={() => (activityOpen = true)}
		/>

		<div class="flex min-h-0 flex-1">
			<!-- Folder pane -->
			<div
				class="{mobileView === 'folders'
					? 'flex'
					: 'hidden'} w-full flex-col md:flex md:w-52 md:flex-none lg:w-60 border-r border-[var(--rs-border)]"
			>
				<div class="flex items-center justify-between px-3 py-2 md:hidden">
					<span class="text-xs font-semibold text-base-content/70">Folders</span>
					<button type="button" class="btn btn-xs btn-ghost" onclick={() => (mobileView = 'list')}>
						Close
					</button>
				</div>
				<MailFolderPane
					{folders}
					foldersLoading={$foldersQuery.isLoading}
					foldersError={$foldersQuery.isError
						? ($foldersQuery.error?.message ?? 'Unknown error')
						: null}
					draftsCount={($draftsQuery.data ?? []).length}
					{selection}
					onSelect={handleSelectFolder}
					onRetryFolders={() => $foldersQuery.refetch()}
				/>
			</div>

			<!-- Message list pane -->
			<div
				class="{mobileView === 'list' ? 'flex' : 'hidden'} w-full flex-col {viewerTarget
					? 'md:hidden'
					: 'md:flex'} md:w-72 md:flex-none lg:flex lg:w-88 xl:w-96 border-r border-[var(--rs-border)]"
			>
				<div class="flex items-center gap-1 border-b border-[var(--rs-border)] px-2 py-1 md:hidden">
					<button
						type="button"
						class="btn btn-xs btn-ghost"
						onclick={() => (mobileView = 'folders')}
					>
						← Folders
					</button>
				</div>
				<MailMessageList
					title={listTitle}
					items={listItems}
					initialLoading={listLoading}
					refreshing={listRefreshing}
					error={listError}
					searchActive={!!search}
					{selectedKey}
					{checkedUids}
					hasMore={listHasMore}
					{loadingMore}
					onOpen={handleOpenItem}
					onToggleCheck={toggleUid}
					onCheckAll={() =>
						(checkedUids = listItems
							.filter((item) => item.kind === 'imap')
							.map((item) => (item.kind === 'imap' ? item.uid : 0)))}
					onClearChecks={() => (checkedUids = [])}
					onImportSelected={() => importMutation.mutate(checkedUids)}
					onArchiveSelected={() => runBulkAction('archive')}
					onTrashSelected={() => runBulkAction('trash')}
					onDeleteSelected={() => runBulkAction('delete')}
					onLoadMore={loadMore}
					onRetry={refreshCurrent}
				/>
			</div>

			<!-- Viewer pane -->
			<div
				class="{mobileView === 'viewer' ? 'flex' : 'hidden'} min-w-0 flex-1 flex-col {viewerTarget
					? 'md:flex'
					: 'md:hidden'} lg:flex"
			>
				<MailMessageViewer
					target={viewerTarget}
					accountId={selectedAccountId}
					accountUsername={selectedAccount?.username ?? null}
					{archiveFolderName}
					{trashFolderName}
					{uidvalidity}
					onBack={handleViewerBack}
					onCompose={openComposeRequest}
					onImapAction={runImapAction}
					onImportUid={(uid) => importMutation.mutate([uid])}
				/>
			</div>
		</div>
	</div>
{/if}

<MailActivityDrawer
	open={activityOpen}
	accountId={selectedAccountId}
	accountName={selectedAccount?.name ?? ''}
	{folders}
	defaultFolder={selection.kind === 'imap' ? selection.name : null}
	onClose={() => (activityOpen = false)}
/>

<MailComposeModal
	open={composeOpen}
	mode={composeMode}
	draftId={composeDraftId}
	initialTo={composeTo}
	initialCc={composeCc}
	initialBcc={composeBcc}
	initialSubject={composeSubject}
	initialBody={composeBody}
	initialAttachments={composeAttachments}
	inReplyToMsgId={composeReplyTo}
	sending={$sendMutation.isPending || $sendDraftMutation.isPending}
	saving={$saveDraftMutation.isPending}
	discarding={$discardDraftMutation.isPending}
	hasSmtp={!!smtpSettings && smtpSettings.is_enabled}
	saveError={composeSaveError}
	onClose={() => (composeOpen = false)}
	onSend={(message) => sendCompose(message)}
	onSave={(message, draftId) =>
		saveDraftMutation.mutateAsync({ message, draftId }).then(() => undefined)}
	onDiscard={(draftId) => discardDraftMutation.mutate(draftId)}
/>
