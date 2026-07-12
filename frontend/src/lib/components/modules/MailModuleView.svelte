<script lang="ts">
	import { goto } from '$app/navigation';
	import { createMutation, createQuery } from '$lib/query-compat';
	import {
		mailApi,
		type MailAccount,
		type MailAccountMessage,
		type MailArchiveJob,
		type MailImportJob,
		type MailFolder,
		type ListMailAccountMessagesResponse,
		type MailMessage
	} from '$lib/api/mail';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageSkeleton from '$lib/components/common/ModulePageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';
	import MailComposeModal from '$lib/components/modules/MailComposeModal.svelte';
	import { toastStore } from '$lib/stores/toast';
	import { Mail, Download, Inbox, RefreshCw, Trash2, Archive, CheckSquare } from 'lucide-svelte';
	import type { ModuleDefinition } from '$lib/modules/registry';

	let { module }: { module: ModuleDefinition } = $props();

	let selectedAccountId = $state<string | null>(null);
	let selectedFolder = $state<string | null>(null);
	let selectedUids = $state<number[]>([]);
	let uidvalidity = $state<number | null>(null);
	let deleteTarget = $state<MailAccount | null>(null);
	let recentImportJobs = $state<MailImportJob[]>([]);

	let accountName = $state('');
	let accountHost = $state('');
	let accountPort = $state('993');
	let accountUsername = $state('');
	let accountPassword = $state('');
	let accountTlsMode = $state<'none' | 'starttls' | 'tls'>('tls');
	let archiveSince = $state('');
	let archiveBefore = $state('');
	let retentionDays = $state('');
	let uploadInput: HTMLInputElement | null = $state(null);
	let composeOpen = $state(false);

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
		queryFn: () => Promise.resolve({ uidvalidity: null, messages: [] }),
		enabled: false
	});

	const archiveJobsQuery = createQuery<MailArchiveJob[]>({
		queryKey: ['mail-archive-jobs', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});

	const importedMessagesQuery = createQuery({
		queryKey: ['mail-messages'],
		queryFn: () => mailApi.listMessages()
	});

	$effect(() => {
		const accounts = $accountsQuery.data ?? [];
		if (!selectedAccountId && accounts.length > 0) selectedAccountId = accounts[0].id;
		if (selectedAccountId && !accounts.some((account) => account.id === selectedAccountId)) {
			selectedAccountId = accounts[0]?.id ?? null;
			selectedFolder = null;
		}
	});

	$effect(() => {
		foldersQuery.setOptions({
			queryKey: ['mail-folders', selectedAccountId],
			queryFn: () => mailApi.listFolders(selectedAccountId!),
			enabled: !!selectedAccountId
		});
		archiveJobsQuery.setOptions({
			queryKey: ['mail-archive-jobs', selectedAccountId],
			queryFn: () => mailApi.listArchiveJobs(selectedAccountId!),
			enabled: !!selectedAccountId
		});
	});

	$effect(() => {
		const folders = $foldersQuery.data ?? [];
		if (!selectedFolder && folders.length > 0) selectedFolder = folders[0].name;
		if (selectedFolder && !folders.some((folder) => folder.name === selectedFolder)) {
			selectedFolder = folders[0]?.name ?? null;
		}
	});

	$effect(() => {
		selectedUids = [];
		accountMessagesQuery.setOptions({
			queryKey: ['mail-account-messages', selectedAccountId, selectedFolder],
			queryFn: () => mailApi.listAccountMessages(selectedAccountId!, selectedFolder!, 100),
			enabled: !!selectedAccountId && !!selectedFolder
		});
	});

	$effect(() => {
		uidvalidity = $accountMessagesQuery.data?.uidvalidity ?? null;
	});

	const createAccountMutation = createMutation({
		mutationFn: () =>
			mailApi.createAccount({
				name: accountName.trim(),
				host: accountHost.trim(),
				port: Number(accountPort),
				username: accountUsername.trim(),
				password: accountPassword,
				tls_mode: accountTlsMode
			}),
		onSuccess: async (account) => {
			accountName = '';
			accountHost = '';
			accountPort = '993';
			accountUsername = '';
			accountPassword = '';
			accountTlsMode = 'tls';
			selectedAccountId = account.id;
			await accountsQuery.refetch();
			toastStore.show('Mail account added', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Account failed', 'error')
	});

	const deleteAccountMutation = createMutation({
		mutationFn: (accountId: string) => mailApi.deleteAccount(accountId),
		onSuccess: async () => {
			deleteTarget = null;
			selectedFolder = null;
			await accountsQuery.refetch();
			toastStore.show('Mail account disconnected', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Delete failed', 'error')
	});

	const testAccountMutation = createMutation({
		mutationFn: (accountId: string) => mailApi.testAccount(accountId),
		onSuccess: () => toastStore.show('Connection succeeded', 'success'),
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Connection failed', 'error')
	});

	const importMutation = createMutation({
		mutationFn: () =>
			mailApi.createImportJob(selectedAccountId!, {
				folder_name: selectedFolder!,
				source_uidvalidity: uidvalidity,
				selected_uids: selectedUids
			}),
		onSuccess: async (job) => {
			selectedUids = [];
			recentImportJobs = [job, ...recentImportJobs.filter((item) => item.id !== job.id)].slice(
				0,
				5
			);
			await importedMessagesQuery.refetch();
			toastStore.show('Import job queued', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Import failed', 'error')
	});

	const refreshImportJobMutation = createMutation({
		mutationFn: (jobId: string) => mailApi.getImportJob(jobId),
		onSuccess: (job) => {
			recentImportJobs = recentImportJobs.map((item) => (item.id === job.id ? job : item));
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Refresh failed', 'error')
	});

	const archiveMutation = createMutation({
		mutationFn: () =>
			mailApi.createArchiveJob(selectedAccountId!, {
				folder_name: selectedFolder!,
				archive_since: archiveSince || null,
				archive_before: archiveBefore || null,
				retention_days: retentionDays ? Number(retentionDays) : null,
				max_retries: 5
			}),
		onSuccess: async () => {
			archiveSince = '';
			archiveBefore = '';
			retentionDays = '';
			await archiveJobsQuery.refetch();
			toastStore.show('Archive job queued', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Archive failed', 'error')
	});

	const cancelArchiveMutation = createMutation({
		mutationFn: (jobId: string) => mailApi.cancelArchiveJob(jobId),
		onSuccess: async () => {
			await archiveJobsQuery.refetch();
			toastStore.show('Archive job cancelled', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Cancel failed', 'error')
	});

	const uploadMutation = createMutation({
		mutationFn: (file: File) => mailApi.uploadMessage(file),
		onSuccess: async () => {
			await importedMessagesQuery.refetch();
			toastStore.show('Mail imported', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Upload failed', 'error')
	});

	const sendMutation = createMutation({
		mutationFn: mailApi.sendMessage,
		onSuccess: () => {
			composeOpen = false;
			toastStore.show('Mail sent', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Send failed', 'error')
	});

	let selectedAccount = $derived(
		($accountsQuery.data ?? []).find((account) => account.id === selectedAccountId) ?? null
	);

	function handleOpenMessage(message: MailMessage) {
		goto(`/modules/mail/messages/${message.id}`);
	}

	function formatAddresses(value: unknown): string {
		if (Array.isArray(value)) return value.join(', ');
		return String(value ?? '');
	}

	function formatBytes(value: number | null | undefined): string {
		if (!value) return '0 B';
		if (value < 1024) return `${value} B`;
		if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
		return `${(value / 1024 / 1024).toFixed(1)} MB`;
	}

	function toggleUid(uid: number) {
		selectedUids = selectedUids.includes(uid)
			? selectedUids.filter((selected) => selected !== uid)
			: [...selectedUids, uid];
	}

	function selectAllVisible(messages: MailAccountMessage[]) {
		selectedUids = messages.map((message) => message.uid);
	}

	function jobProgress(job: MailArchiveJob): string {
		return `${job.processed_messages}/${job.total_messages} processed`;
	}

	function importJobProgress(job: MailImportJob): string {
		return `${job.processed_messages}/${job.total_messages} processed`;
	}

	function handleUploadChange(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (file) uploadMutation.mutate(file);
		input.value = '';
	}
</script>

<ModulePageShell title="Mail" subtitle={module.description}>
	<div slot="primaryAction" class="flex flex-wrap gap-2">
		<button class="btn gap-2 btn-sm btn-outline" onclick={() => (composeOpen = true)}>
			<Mail size={14} />
			<span>Compose</span>
		</button>
		<button class="btn gap-2 btn-sm btn-outline" onclick={() => importedMessagesQuery.refetch()}>
			<RefreshCw size={14} />
			<span>Refresh imported</span>
		</button>
		<input
			bind:this={uploadInput}
			class="hidden"
			type="file"
			accept=".eml,message/rfc822"
			onchange={handleUploadChange}
		/>
		<button
			class="btn gap-2 btn-sm btn-primary"
			disabled={$uploadMutation.isPending}
			onclick={() => uploadInput?.click()}
		>
			<Download size={14} />
			<span>{$uploadMutation.isPending ? 'Uploading...' : 'Upload .eml'}</span>
		</button>
	</div>

	<div class="grid grid-cols-1 gap-4 xl:grid-cols-[320px_minmax(0,1fr)]">
		<section class="flex flex-col gap-4">
			<div class="rounded-lg border border-base-300 bg-base-100 p-4">
				<h2 class="mb-3 text-sm font-semibold">Accounts</h2>
				<form
					class="flex flex-col gap-2"
					onsubmit={(event) => {
						event.preventDefault();
						createAccountMutation.mutate();
					}}
				>
					<input
						class="input input-sm input-bordered"
						placeholder="Account name"
						bind:value={accountName}
						required
					/>
					<input
						class="input input-sm input-bordered"
						placeholder="IMAP host"
						bind:value={accountHost}
						required
					/>
					<div class="grid grid-cols-[1fr_120px] gap-2">
						<input
							class="input input-sm input-bordered"
							placeholder="Username"
							bind:value={accountUsername}
							required
						/>
						<input
							class="input input-sm input-bordered"
							type="number"
							min="1"
							max="65535"
							bind:value={accountPort}
							required
						/>
					</div>
					<input
						class="input input-sm input-bordered"
						type="password"
						placeholder="Password"
						bind:value={accountPassword}
						autocomplete="new-password"
						required
					/>
					<select class="select select-sm select-bordered" bind:value={accountTlsMode}>
						<option value="tls">TLS</option>
						<option value="starttls">STARTTLS</option>
						<option value="none">No TLS</option>
					</select>
					<p class="text-xs text-base-content/55">
						Passwords are saved server-side and never shown after save.
					</p>
					<button
						class="btn btn-sm btn-primary"
						type="submit"
						disabled={$createAccountMutation.isPending}
					>
						Add account
					</button>
				</form>
			</div>

			{#if $accountsQuery.isLoading}
				<ModulePageSkeleton />
			{:else if $accountsQuery.isError}
				<ErrorState
					title="Failed to load accounts"
					message={$accountsQuery.error?.message || 'Unknown error'}
					onRetry={() => accountsQuery.refetch()}
				/>
			{:else if ($accountsQuery.data ?? []).length === 0}
				<EmptyState
					icon="✉️"
					title="No accounts"
					description="Add an IMAP account to browse folders and import messages."
				/>
			{:else}
				<div class="flex flex-col gap-2">
					{#each $accountsQuery.data ?? [] as account}
						<button
							type="button"
							class="rounded-lg border p-3 text-left {selectedAccountId === account.id
								? 'border-primary bg-primary/10'
								: 'border-base-300 bg-base-100'}"
							onclick={() => {
								selectedAccountId = account.id;
								selectedFolder = null;
							}}
						>
							<div class="truncate text-sm font-semibold">{account.name}</div>
							<div class="truncate text-xs text-base-content/60">
								{account.username} · {account.host}:{account.port}
							</div>
							{#if account.last_error}
								<div class="mt-1 truncate text-xs text-error">{account.last_error}</div>
							{/if}
						</button>
					{/each}
				</div>
			{/if}

			{#if selectedAccount}
				<div class="flex gap-2">
					<button
						class="btn btn-sm btn-outline flex-1"
						onclick={() => testAccountMutation.mutate(selectedAccount!.id)}
					>
						Test
					</button>
					<button
						class="btn btn-sm btn-error btn-outline"
						onclick={() => (deleteTarget = selectedAccount)}
					>
						<Trash2 size={14} />
					</button>
				</div>
			{/if}
		</section>

		<section class="grid grid-cols-1 gap-4 2xl:grid-cols-[260px_minmax(0,1fr)]">
			<div class="rounded-lg border border-base-300 bg-base-100 p-4">
				<h2 class="mb-3 text-sm font-semibold">Folders</h2>
				{#if !selectedAccountId}
					<p class="text-sm text-base-content/60">Select an account.</p>
				{:else if $foldersQuery.isLoading}
					<ModulePageSkeleton />
				{:else if $foldersQuery.isError}
					<ErrorState
						title="Failed to load folders"
						message={$foldersQuery.error?.message || 'Unknown error'}
						onRetry={() => foldersQuery.refetch()}
					/>
				{:else if ($foldersQuery.data ?? []).length === 0}
					<EmptyState
						icon="📁"
						title="No folders"
						description="This account did not return folders."
					/>
				{:else}
					<div class="flex max-h-[520px] flex-col gap-1 overflow-auto">
						{#each $foldersQuery.data ?? [] as folder}
							<button
								type="button"
								class="truncate rounded-md px-3 py-2 text-left text-sm {selectedFolder ===
								folder.name
									? 'bg-primary text-primary-content'
									: 'hover:bg-base-200'}"
								title={folder.name}
								onclick={() => (selectedFolder = folder.name)}
							>
								{folder.name}
							</button>
						{/each}
					</div>
				{/if}
			</div>

			<div class="flex flex-col gap-4">
				<div class="rounded-lg border border-base-300 bg-base-100 p-4">
					<div class="mb-3 flex flex-wrap items-center justify-between gap-2">
						<div>
							<h2 class="text-sm font-semibold">Mailbox</h2>
							<p class="text-xs text-base-content/60">
								{selectedAccount?.name ?? 'No account'}{selectedFolder
									? ` / ${selectedFolder}`
									: ''}
							</p>
						</div>
						<div class="flex gap-2">
							<button
								class="btn btn-sm btn-outline"
								disabled={!$accountMessagesQuery.data?.messages?.length}
								onclick={() => selectAllVisible($accountMessagesQuery.data!.messages)}
							>
								<CheckSquare size={14} /> Select visible
							</button>
							<button
								class="btn btn-sm btn-primary"
								disabled={selectedUids.length === 0 || $importMutation.isPending}
								onclick={() => importMutation.mutate()}
							>
								Import {selectedUids.length || ''}
							</button>
						</div>
					</div>
					{#if !selectedFolder}
						<EmptyState
							icon="📬"
							title="Select a folder"
							description="Choose a folder to load message summaries."
						/>
					{:else if $accountMessagesQuery.isLoading}
						<ModulePageSkeleton />
					{:else if $accountMessagesQuery.isError}
						<ErrorState
							title="Failed to load messages"
							message={$accountMessagesQuery.error?.message || 'Unknown error'}
							onRetry={() => accountMessagesQuery.refetch()}
						/>
					{:else if ($accountMessagesQuery.data?.messages ?? []).length === 0}
						<EmptyState
							icon="📭"
							title="Empty folder"
							description="This folder has no messages to import."
						/>
					{:else}
						<div class="max-h-[520px] divide-y divide-base-300 overflow-auto">
							{#each $accountMessagesQuery.data?.messages ?? [] as message}
								<label class="grid cursor-pointer grid-cols-[auto_minmax(0,1fr)_auto] gap-3 py-3">
									<input
										class="checkbox checkbox-sm mt-1"
										type="checkbox"
										checked={selectedUids.includes(message.uid)}
										onchange={() => toggleUid(message.uid)}
									/>
									<span class="min-w-0">
										<span class="block truncate text-sm font-medium"
											>{message.subject || '(no subject)'}</span
										>
										<span class="block truncate text-xs text-base-content/60"
											>{message.from_name || message.from_address || 'Unknown sender'}</span
										>
									</span>
									<span class="text-right text-xs text-base-content/55">
										{message.sent_at
											? new Date(message.sent_at).toLocaleDateString()
											: 'No date'}<br />
										{formatBytes(message.size_bytes)}
									</span>
								</label>
							{/each}
						</div>
						<p class="mt-2 text-xs text-base-content/50">
							Showing the newest 100 messages to avoid loading huge folders.
						</p>
					{/if}
				</div>

				{#if recentImportJobs.length > 0}
					<div class="rounded-lg border border-base-300 bg-base-100 p-4">
						<h2 class="mb-3 text-sm font-semibold">Recent imports</h2>
						<div class="flex flex-col gap-2">
							{#each recentImportJobs as job}
								<div class="rounded-md border border-base-300 p-3">
									<div class="flex flex-wrap items-center justify-between gap-2">
										<div class="min-w-0">
											<div class="truncate text-sm font-medium">{job.folder_name}</div>
											<div class="text-xs text-base-content/60">
												{job.status} · {importJobProgress(job)} · failed {job.failed_messages}
											</div>
										</div>
										<button
											type="button"
											class="btn btn-xs btn-outline"
											onclick={() => refreshImportJobMutation.mutate(job.id)}
										>
											Refresh
										</button>
									</div>
									{#if job.last_error}
										<p class="mt-1 text-xs text-error">{job.last_error}</p>
									{/if}
								</div>
							{/each}
						</div>
					</div>
				{/if}

				<div class="rounded-lg border border-base-300 bg-base-100 p-4">
					<h2 class="mb-3 flex items-center gap-2 text-sm font-semibold">
						<Archive size={15} /> Archive jobs
					</h2>
					<div class="mb-4 grid grid-cols-1 gap-2 md:grid-cols-[1fr_1fr_120px_auto]">
						<input
							class="input input-sm input-bordered"
							type="date"
							bind:value={archiveSince}
							aria-label="Archive since"
						/>
						<input
							class="input input-sm input-bordered"
							type="date"
							bind:value={archiveBefore}
							aria-label="Archive before"
						/>
						<input
							class="input input-sm input-bordered"
							type="number"
							min="1"
							max="36500"
							placeholder="Retention"
							bind:value={retentionDays}
						/>
						<button
							class="btn btn-sm btn-outline"
							disabled={!selectedFolder || $archiveMutation.isPending}
							onclick={() => archiveMutation.mutate()}
						>
							Queue archive
						</button>
					</div>
					{#if $archiveJobsQuery.isLoading}
						<ModulePageSkeleton />
					{:else if $archiveJobsQuery.isError}
						<ErrorState
							title="Failed to load archive jobs"
							message={$archiveJobsQuery.error?.message || 'Unknown error'}
							onRetry={() => archiveJobsQuery.refetch()}
						/>
					{:else if ($archiveJobsQuery.data ?? []).length === 0}
						<p class="text-sm text-base-content/60">No archive jobs for this account.</p>
					{:else}
						<div class="flex flex-col gap-2">
							{#each $archiveJobsQuery.data ?? [] as job}
								<div class="rounded-md border border-base-300 p-3">
									<div class="flex flex-wrap items-center justify-between gap-2">
										<div class="min-w-0">
											<div class="truncate text-sm font-medium">{job.folder_name}</div>
											<div class="text-xs text-base-content/60">
												{job.status} · {jobProgress(job)} · retries {job.retry_count}/{job.max_retries}
											</div>
										</div>
										{#if ['pending', 'running'].includes(job.status)}
											<button
												type="button"
												class="btn btn-xs btn-outline"
												onclick={() => cancelArchiveMutation.mutate(job.id)}
											>
												Cancel
											</button>
										{/if}
									</div>
									{#if job.last_error}
										<p class="mt-1 text-xs text-error">{job.last_error}</p>
									{/if}
								</div>
							{/each}
						</div>
					{/if}
				</div>

				<div class="rounded-lg border border-base-300 bg-base-100 p-4">
					<h2 class="mb-3 flex items-center gap-2 text-sm font-semibold">
						<Inbox size={15} /> Imported RustShare mail
					</h2>
					{#if $importedMessagesQuery.isLoading}
						<ModulePageSkeleton />
					{:else if $importedMessagesQuery.isError}
						<ErrorState
							title="Failed to load imported mail"
							message={$importedMessagesQuery.error?.message || 'Unknown error'}
							onRetry={() => importedMessagesQuery.refetch()}
						/>
					{:else if !$importedMessagesQuery.data || $importedMessagesQuery.data.length === 0}
						<EmptyState
							icon={'✉️'}
							title={module.ui.page.emptyStateTitle}
							description={module.ui.page.emptyStateDescription}
							actionLabel={module.ui.page.primaryAction?.label}
							onAction={() => goto('/files')}
						/>
					{:else}
						<div class="flex flex-col gap-2">
							{#each $importedMessagesQuery.data as message}
								<button
									type="button"
									class="flex items-center gap-4 rounded-lg border border-base-300/70 p-3 text-left hover:border-primary/40"
									onclick={() => handleOpenMessage(message)}
								>
									<div
										class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary"
									>
										<Mail size={20} />
									</div>
									<div class="min-w-0 flex-1">
										<span class="block truncate text-sm font-semibold"
											>{message.subject || '(no subject)'}</span
										>
										<span class="block truncate text-xs text-base-content/55"
											>{message.from_name || message.from_address || 'Unknown sender'} · {message.sent_at
												? new Date(message.sent_at).toLocaleString()
												: `imported ${new Date(message.imported_at).toLocaleString()}`}</span
										>
										<span class="block truncate text-xs text-base-content/45"
											>To: {formatAddresses(message.to_addresses)}</span
										>
									</div>
									{#if message.has_attachments}
										<span class="badge badge-sm badge-ghost">attachments</span>
									{/if}
								</button>
							{/each}
						</div>
					{/if}
				</div>
			</div>
		</section>
	</div>
</ModulePageShell>

<ConfirmModal
	open={deleteTarget !== null}
	title="Disconnect mail account"
	message="This removes the IMAP account connection. Imported RustShare mail artifacts remain available."
	confirmLabel="Disconnect"
	danger
	onConfirm={() => deleteTarget && deleteAccountMutation.mutate(deleteTarget.id)}
	onCancel={() => (deleteTarget = null)}
/>

<MailComposeModal
	open={composeOpen}
	sending={$sendMutation.isPending}
	onClose={() => (composeOpen = false)}
	onSend={(message) => sendMutation.mutate(message)}
/>
