<script lang="ts">
	import { onMount } from 'svelte';
	import { createQuery } from '$lib/query-compat';
	import { mailApi, type MailAccount, type MailFolder, type MailSmtpSettings } from '$lib/api/mail';
	import CollapsibleSection from '$lib/settings/CollapsibleSection.svelte';
	import MailArchivePanel from '$lib/components/modules/mail/MailArchivePanel.svelte';
	import {
		mailAccountStatus,
		mailAccountStatusLabel
	} from '$lib/components/modules/mail/mail-types';
	import { toastStore } from '$lib/stores/toast';
	import { KeyRound, Mail, Plus, Trash2 } from 'lucide-svelte';

	type Provider = 'custom' | 'gmail';

	const PROVIDER_PRESETS: Record<
		Provider,
		{
			imap: { host: string; port: number; tls_mode: 'tls' };
			smtp: { host: string; port: number; tls_mode: 'starttls' };
		} | null
	> = {
		custom: null,
		gmail: {
			imap: { host: 'imap.gmail.com', port: 993, tls_mode: 'tls' },
			smtp: { host: 'smtp.gmail.com', port: 587, tls_mode: 'starttls' }
		}
	};

	let mailAccounts = $state<MailAccount[]>([]);
	let mailAccountsLoading = $state(true);
	let selectedMailAccountId = $state<string | null>(null);
	let selectedSmtp = $state<MailSmtpSettings | null>(null);
	let smtpLoaded = $state(false);
	let showAddAccountForm = $state(false);
	let addingAccount = $state(false);
	let saving = $state(false);
	let testingImap = $state(false);
	let testingSmtp = $state(false);
	let replaceImapPassword = $state(false);
	let replaceSmtpPassword = $state(false);
	let provider = $state<Provider>('custom');
	let pendingSmtpPreset = $state<{ host: string; port: number; tls_mode: 'starttls' } | null>(null);

	let imapForm = $state({
		name: '',
		host: '',
		port: 993,
		username: '',
		password: '',
		tls_mode: 'tls' as 'none' | 'starttls' | 'tls',
		is_enabled: true
	});
	let smtpForm = $state({
		host: '',
		port: 587,
		username: '',
		password: '',
		tls_mode: 'starttls' as 'starttls' | 'tls',
		from_address: '',
		from_name: '',
		reply_to: '',
		sent_folder: '',
		is_enabled: true
	});

	const foldersQuery = createQuery<MailFolder[]>({
		queryKey: ['mail-folders', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});

	$effect(() => {
		foldersQuery.setOptions({
			queryKey: ['mail-folders', selectedMailAccountId],
			queryFn: () => mailApi.listFolders(selectedMailAccountId!),
			enabled: !!selectedMailAccountId && !showAddAccountForm
		});
	});

	let selectedAccount = $derived(
		mailAccounts.find((account) => account.id === selectedMailAccountId) ?? null
	);

	let accountStatus = $derived(
		selectedAccount
			? mailAccountStatus(
					selectedAccount,
					smtpLoaded ? !!(selectedSmtp && selectedSmtp.is_enabled) : null
				)
			: 'untested'
	);

	function statusDotClass(account: MailAccount): string {
		if (!account.is_enabled) return 'bg-base-content/25';
		if (account.last_error) return 'bg-error';
		if (!account.last_connected_at) return 'bg-warning';
		return 'bg-success';
	}

	function statusChipClass(status: string): string {
		switch (status) {
			case 'connected':
				return 'badge-success';
			case 'partial':
				return 'badge-warning';
			case 'failed':
				return 'badge-error';
			case 'disabled':
				return 'badge-ghost';
			default:
				return 'badge-warning';
		}
	}

	function applyProvider(next: Provider) {
		provider = next;
		const preset = PROVIDER_PRESETS[next];
		if (preset) {
			imapForm.host = preset.imap.host;
			imapForm.port = preset.imap.port;
			imapForm.tls_mode = preset.imap.tls_mode;
			pendingSmtpPreset = preset.smtp;
		} else {
			pendingSmtpPreset = null;
		}
	}

	async function loadMailAccounts() {
		mailAccountsLoading = true;
		try {
			mailAccounts = await mailApi.listAccounts();
			if (mailAccounts.length > 0 && !selectedMailAccountId) {
				await selectMailAccount(mailAccounts[0].id);
			}
		} catch (error) {
			toastStore.show(
				error instanceof Error ? error.message : 'Failed to load mail accounts',
				'error'
			);
		} finally {
			mailAccountsLoading = false;
		}
	}

	function resetSmtpForm(account: MailAccount | null, preset: typeof pendingSmtpPreset = null) {
		smtpForm = {
			host: preset?.host ?? '',
			port: preset?.port ?? 587,
			username: account?.username ?? '',
			password: '',
			tls_mode: preset?.tls_mode ?? 'starttls',
			from_address: account?.username ?? '',
			from_name: '',
			reply_to: '',
			sent_folder: '',
			is_enabled: true
		};
	}

	async function selectMailAccount(id: string) {
		selectedMailAccountId = id;
		showAddAccountForm = false;
		replaceImapPassword = false;
		replaceSmtpPassword = false;
		const account = mailAccounts.find((a) => a.id === id);
		if (account) {
			imapForm = {
				name: account.name,
				host: account.host,
				port: account.port,
				username: account.username,
				password: '',
				tls_mode: account.tls_mode as 'none' | 'starttls' | 'tls',
				is_enabled: account.is_enabled
			};
		}
		selectedSmtp = null;
		smtpLoaded = false;
		try {
			const smtp = await mailApi.getSmtpSettings(id);
			selectedSmtp = smtp;
			if (smtp) {
				smtpForm = {
					host: smtp.host,
					port: smtp.port,
					username: smtp.username,
					password: '',
					tls_mode: smtp.tls_mode,
					from_address: smtp.from_address,
					from_name: smtp.from_name ?? '',
					reply_to: smtp.reply_to ?? '',
					sent_folder: smtp.sent_folder ?? '',
					is_enabled: smtp.is_enabled
				};
			} else {
				resetSmtpForm(account ?? null, pendingSmtpPreset);
			}
		} catch (error) {
			toastStore.show(
				error instanceof Error ? error.message : 'Failed to load SMTP settings',
				'error'
			);
			resetSmtpForm(account ?? null, pendingSmtpPreset);
		} finally {
			smtpLoaded = true;
		}
	}

	function openAddAccountForm() {
		showAddAccountForm = true;
		selectedMailAccountId = null;
		provider = 'custom';
		pendingSmtpPreset = null;
		imapForm = {
			name: '',
			host: '',
			port: 993,
			username: '',
			password: '',
			tls_mode: 'tls',
			is_enabled: true
		};
	}

	async function handleAddMailAccount() {
		addingAccount = true;
		try {
			const account = await mailApi.createAccount({
				name: imapForm.name.trim(),
				host: imapForm.host.trim(),
				port: Number(imapForm.port),
				username: imapForm.username.trim(),
				password: imapForm.password,
				tls_mode: imapForm.tls_mode
			});
			toastStore.show('Mail account added', 'success');
			showAddAccountForm = false;
			await loadMailAccounts();
			await selectMailAccount(account.id);
			pendingSmtpPreset = null;
		} catch (error) {
			toastStore.show(error instanceof Error ? error.message : 'Failed to add account', 'error');
		} finally {
			addingAccount = false;
		}
	}

	async function handleSaveAll() {
		if (!selectedMailAccountId) return;
		saving = true;
		try {
			await mailApi.updateAccount(selectedMailAccountId, {
				name: imapForm.name.trim(),
				host: imapForm.host.trim(),
				port: Number(imapForm.port),
				username: imapForm.username.trim(),
				password: replaceImapPassword && imapForm.password ? imapForm.password : undefined,
				tls_mode: imapForm.tls_mode,
				is_enabled: imapForm.is_enabled
			});
			if (smtpForm.host.trim()) {
				selectedSmtp = await mailApi.updateSmtpSettings(selectedMailAccountId, {
					host: smtpForm.host.trim(),
					port: Number(smtpForm.port),
					username: smtpForm.username.trim(),
					password: replaceSmtpPassword && smtpForm.password ? smtpForm.password : null,
					tls_mode: smtpForm.tls_mode,
					from_address: smtpForm.from_address.trim(),
					from_name: smtpForm.from_name.trim() || null,
					reply_to: smtpForm.reply_to.trim() || null,
					sent_folder: smtpForm.sent_folder.trim() || null,
					is_enabled: smtpForm.is_enabled
				});
			}
			imapForm.password = '';
			smtpForm.password = '';
			replaceImapPassword = false;
			replaceSmtpPassword = false;
			toastStore.show('Mail settings saved', 'success');
			await loadMailAccounts();
		} catch (error) {
			toastStore.show(
				error instanceof Error ? error.message : 'Failed to save mail settings',
				'error'
			);
		} finally {
			saving = false;
		}
	}

	async function handleTestImap() {
		if (!selectedMailAccountId) return;
		testingImap = true;
		try {
			await mailApi.testAccount(selectedMailAccountId);
			toastStore.show('Incoming mail (IMAP) connection successful', 'success');
			await loadMailAccounts();
		} catch (error) {
			toastStore.show(error instanceof Error ? error.message : 'IMAP connection failed', 'error');
		} finally {
			testingImap = false;
		}
	}

	async function handleTestSmtp() {
		if (!selectedMailAccountId) return;
		testingSmtp = true;
		try {
			await mailApi.testSmtpConnection(selectedMailAccountId);
			toastStore.show('Outgoing mail (SMTP) connection successful', 'success');
		} catch (error) {
			toastStore.show(error instanceof Error ? error.message : 'SMTP connection failed', 'error');
		} finally {
			testingSmtp = false;
		}
	}

	async function handleDeleteSmtp() {
		if (!selectedMailAccountId) return;
		if (!confirm('Delete SMTP settings for this account? You will not be able to send mail.'))
			return;
		try {
			await mailApi.deleteSmtpSettings(selectedMailAccountId);
			selectedSmtp = null;
			resetSmtpForm(selectedAccount);
			toastStore.show('SMTP settings deleted', 'success');
		} catch (error) {
			toastStore.show(
				error instanceof Error ? error.message : 'Failed to delete SMTP settings',
				'error'
			);
		}
	}

	async function handleDeleteAccount(id: string) {
		if (
			!confirm(
				'Are you sure you want to disconnect this mail account? Imported mail artifacts will remain.'
			)
		)
			return;
		try {
			await mailApi.deleteAccount(id);
			toastStore.show('Mail account disconnected', 'success');
			if (selectedMailAccountId === id) {
				selectedMailAccountId = null;
				selectedSmtp = null;
			}
			await loadMailAccounts();
		} catch (error) {
			toastStore.show(error instanceof Error ? error.message : 'Failed to delete account', 'error');
		}
	}

	onMount(loadMailAccounts);
</script>

<div>
	<div class="mb-4 space-y-1">
		<h2 class="text-lg font-semibold text-base-content">Mail settings</h2>
		<p class="text-sm text-base-content/60">
			Manage your incoming IMAP and outgoing SMTP accounts.
		</p>
	</div>

	<div class="grid grid-cols-1 gap-4 md:grid-cols-[260px_minmax(0,1fr)]">
		<!-- Account list -->
		<div
			class="flex h-fit flex-col rounded-lg border border-[var(--rs-border)] bg-[var(--rs-surface-raised)]"
		>
			<div class="border-b border-[var(--rs-border)] px-3 py-2.5">
				<h3 class="text-xs font-semibold text-base-content/70">Mail accounts</h3>
			</div>
			<div class="flex flex-col gap-0.5 p-2">
				{#if mailAccountsLoading}
					<div class="flex flex-col gap-1.5 p-1" aria-label="Loading accounts">
						<div class="skeleton h-10 w-full"></div>
						<div class="skeleton h-10 w-full opacity-70"></div>
					</div>
				{:else if mailAccounts.length === 0}
					<p class="px-2 py-3 text-xs text-base-content/50">No mail account configured.</p>
				{:else}
					{#each mailAccounts as account}
						<button
							type="button"
							class="flex items-start gap-2 rounded-md px-2.5 py-2 text-left transition-colors {selectedMailAccountId ===
							account.id
								? 'bg-base-200'
								: 'hover:bg-base-200/70'}"
							aria-current={selectedMailAccountId === account.id ? 'true' : undefined}
							onclick={() => selectMailAccount(account.id)}
						>
							<span
								class="mt-1.5 h-2 w-2 shrink-0 rounded-full {statusDotClass(account)}"
								title={account.last_error ? 'Connection failed' : 'Account status'}
							></span>
							<span class="min-w-0 flex-1">
								<span class="block truncate text-sm font-medium text-base-content">
									{account.name}
								</span>
								<span class="block truncate text-xs text-base-content/55">{account.username}</span>
							</span>
						</button>
					{/each}
				{/if}
			</div>
			<div class="border-t border-[var(--rs-border)] p-2">
				<button
					type="button"
					class="btn btn-sm btn-outline w-full gap-1.5"
					onclick={openAddAccountForm}
				>
					<Plus size={14} /> Add account
				</button>
			</div>
		</div>

		<!-- Details -->
		<div class="rounded-lg border border-[var(--rs-border)] bg-[var(--rs-surface-raised)]">
			{#if showAddAccountForm}
				<div class="border-b border-[var(--rs-border)] px-4 py-3">
					<h3 class="text-sm font-semibold text-base-content">Add mail account</h3>
					<p class="mt-0.5 text-xs text-base-content/55">
						Configure a new IMAP account. Outgoing mail can be added afterwards.
					</p>
				</div>
				<form
					class="flex flex-col gap-4 p-4"
					onsubmit={(event) => {
						event.preventDefault();
						handleAddMailAccount();
					}}
				>
					<div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
						<div class="form-control">
							<label class="label py-1 text-xs font-semibold" for="add-provider">Provider</label>
							<select
								id="add-provider"
								class="select select-sm select-bordered"
								value={provider}
								onchange={(event) =>
									applyProvider((event.target as HTMLSelectElement).value as Provider)}
							>
								<option value="custom">Custom IMAP server</option>
								<option value="gmail">Gmail</option>
							</select>
						</div>
						<div class="form-control">
							<label class="label py-1 text-xs font-semibold" for="add-name">Account name</label>
							<input
								id="add-name"
								class="input input-sm input-bordered"
								placeholder="My Email"
								bind:value={imapForm.name}
								required
							/>
						</div>
					</div>
					<div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
						<div class="form-control">
							<label class="label py-1 text-xs font-semibold" for="add-username">
								Email address / username
							</label>
							<input
								id="add-username"
								class="input input-sm input-bordered"
								placeholder="user@example.com"
								bind:value={imapForm.username}
								required
							/>
						</div>
						<div class="form-control">
							<label class="label py-1 text-xs font-semibold" for="add-password">
								Password / app password
							</label>
							<input
								id="add-password"
								type="password"
								class="input input-sm input-bordered"
								bind:value={imapForm.password}
								required
								autocomplete="new-password"
							/>
						</div>
					</div>

					<CollapsibleSection title="Advanced server settings" defaultOpen={provider === 'custom'}>
						<div class="grid grid-cols-1 gap-4 sm:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="add-host">IMAP host</label>
								<input
									id="add-host"
									class="input input-sm input-bordered"
									placeholder="imap.example.com"
									bind:value={imapForm.host}
									required
								/>
							</div>
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="add-port">Port</label>
								<input
									id="add-port"
									type="number"
									class="input input-sm input-bordered"
									bind:value={imapForm.port}
									required
								/>
							</div>
						</div>
						<div class="form-control mt-4">
							<label class="label py-1 text-xs font-semibold" for="add-tls">Security</label>
							<select
								id="add-tls"
								class="select select-sm select-bordered"
								bind:value={imapForm.tls_mode}
							>
								<option value="tls">SSL/TLS</option>
							</select>
						</div>
					</CollapsibleSection>

					<div
						class="sticky bottom-0 -mx-4 -mb-4 flex justify-end gap-2 rounded-b-lg border-t border-[var(--rs-border)] bg-[var(--rs-surface-raised)] px-4 py-3"
					>
						<button
							type="button"
							class="btn btn-sm btn-outline"
							onclick={() => (showAddAccountForm = false)}
						>
							Cancel
						</button>
						<button type="submit" class="btn btn-sm btn-primary" disabled={addingAccount}>
							{#if addingAccount}<span class="loading loading-spinner loading-xs"></span>{/if}
							Save account
						</button>
					</div>
				</form>
			{:else if selectedAccount}
				<!-- Account header -->
				<div class="border-b border-[var(--rs-border)] px-4 py-3">
					<div class="flex flex-wrap items-center gap-x-3 gap-y-1">
						<h3 class="min-w-0 truncate text-sm font-semibold text-base-content">
							{selectedAccount.name}
						</h3>
						<span class="badge badge-sm {statusChipClass(accountStatus)}">
							{mailAccountStatusLabel(accountStatus)}
						</span>
					</div>
					<p class="mt-0.5 truncate text-xs text-base-content/55">
						{selectedAccount.username}
						{#if selectedAccount.last_connected_at}
							· Last connected {new Date(selectedAccount.last_connected_at).toLocaleString()}
						{/if}
					</p>
					{#if selectedAccount.last_error}
						<p class="mt-1 truncate text-xs text-error" title={selectedAccount.last_error}>
							{selectedAccount.last_error}
						</p>
					{/if}
				</div>

				<div class="px-4">
					<CollapsibleSection
						title="General"
						description="Account identity and availability"
						defaultOpen={true}
					>
						<div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="mail-account-name">
									Account name
								</label>
								<input
									id="mail-account-name"
									class="input input-sm input-bordered"
									bind:value={imapForm.name}
								/>
							</div>
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="mail-account-display">
									Display name (sender)
								</label>
								<input
									id="mail-account-display"
									class="input input-sm input-bordered"
									placeholder="My Display Name"
									bind:value={smtpForm.from_name}
								/>
							</div>
						</div>
						<label class="mt-3 flex cursor-pointer items-center gap-2">
							<input
								type="checkbox"
								class="checkbox checkbox-sm"
								bind:checked={imapForm.is_enabled}
							/>
							<span class="text-xs text-base-content/70">Account enabled</span>
						</label>
					</CollapsibleSection>

					<CollapsibleSection
						title="Incoming mail (IMAP)"
						description="Server, credentials, and security for reading mail"
						defaultOpen={true}
					>
						<div class="grid grid-cols-1 gap-4 sm:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="imap-host">IMAP host</label>
								<input
									id="imap-host"
									class="input input-sm input-bordered"
									bind:value={imapForm.host}
								/>
							</div>
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="imap-port">Port</label>
								<input
									id="imap-port"
									type="number"
									class="input input-sm input-bordered"
									bind:value={imapForm.port}
								/>
							</div>
						</div>
						<div class="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="imap-security">Security</label>
								<select
									id="imap-security"
									class="select select-sm select-bordered"
									bind:value={imapForm.tls_mode}
								>
									<option value="tls">SSL/TLS</option>
								</select>
							</div>
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="imap-username">
									IMAP username
								</label>
								<input
									id="imap-username"
									class="input input-sm input-bordered"
									bind:value={imapForm.username}
								/>
							</div>
						</div>
						<div class="form-control mt-4">
							<span class="label py-1 text-xs font-semibold">Password / app password</span>
							{#if !replaceImapPassword}
								<div class="flex items-center gap-2">
									<span
										class="flex h-8 flex-1 items-center gap-2 rounded-md border border-[var(--rs-border)] bg-base-200/50 px-3 text-xs text-base-content/60"
									>
										<KeyRound size={13} /> Saved
									</span>
									<button
										type="button"
										class="btn btn-sm btn-outline"
										onclick={() => (replaceImapPassword = true)}
									>
										Replace password
									</button>
								</div>
							{:else}
								<div class="flex items-center gap-2">
									<input
										type="password"
										class="input input-sm input-bordered flex-1"
										placeholder="Enter new password"
										aria-label="New IMAP password"
										bind:value={imapForm.password}
										autocomplete="new-password"
									/>
									<button
										type="button"
										class="btn btn-sm btn-ghost"
										onclick={() => {
											replaceImapPassword = false;
											imapForm.password = '';
										}}
									>
										Cancel
									</button>
								</div>
								<p class="mt-1 text-2xs text-base-content/50">
									The new password is stored only after you choose Save changes.
								</p>
							{/if}
						</div>
					</CollapsibleSection>

					<CollapsibleSection
						title="Outgoing mail (SMTP)"
						description="Server, credentials, and sender identity for sending mail"
						defaultOpen={!selectedSmtp}
					>
						<div class="grid grid-cols-1 gap-4 sm:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="smtp-host">SMTP host</label>
								<input
									id="smtp-host"
									class="input input-sm input-bordered"
									placeholder="smtp.example.com"
									bind:value={smtpForm.host}
								/>
							</div>
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="smtp-port">Port</label>
								<input
									id="smtp-port"
									type="number"
									class="input input-sm input-bordered"
									bind:value={smtpForm.port}
								/>
							</div>
						</div>
						<div class="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="smtp-username">
									SMTP username
								</label>
								<input
									id="smtp-username"
									class="input input-sm input-bordered"
									placeholder="user@example.com"
									bind:value={smtpForm.username}
								/>
							</div>
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="smtp-tls">Security</label>
								<select
									id="smtp-tls"
									class="select select-sm select-bordered"
									bind:value={smtpForm.tls_mode}
								>
									<option value="starttls">STARTTLS</option>
									<option value="tls">SSL/TLS</option>
								</select>
							</div>
						</div>
						<div class="form-control mt-4">
							<span class="label py-1 text-xs font-semibold">Password / app password</span>
							{#if selectedSmtp && !replaceSmtpPassword}
								<div class="flex items-center gap-2">
									<span
										class="flex h-8 flex-1 items-center gap-2 rounded-md border border-[var(--rs-border)] bg-base-200/50 px-3 text-xs text-base-content/60"
									>
										<KeyRound size={13} /> Saved
									</span>
									<button
										type="button"
										class="btn btn-sm btn-outline"
										onclick={() => (replaceSmtpPassword = true)}
									>
										Replace password
									</button>
								</div>
							{:else}
								<div class="flex items-center gap-2">
									<input
										type="password"
										class="input input-sm input-bordered flex-1"
										placeholder={selectedSmtp
											? 'Enter new password'
											: 'Password for the SMTP server'}
										aria-label="SMTP password"
										bind:value={smtpForm.password}
										required={!selectedSmtp}
										autocomplete="new-password"
									/>
									{#if selectedSmtp}
										<button
											type="button"
											class="btn btn-sm btn-ghost"
											onclick={() => {
												replaceSmtpPassword = false;
												smtpForm.password = '';
											}}
										>
											Cancel
										</button>
									{/if}
								</div>
							{/if}
						</div>
						<div class="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="smtp-from">From address</label>
								<input
									id="smtp-from"
									type="email"
									class="input input-sm input-bordered"
									bind:value={smtpForm.from_address}
								/>
							</div>
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="smtp-replyto">
									Reply-To address
								</label>
								<input
									id="smtp-replyto"
									type="email"
									class="input input-sm input-bordered"
									placeholder="reply@example.com"
									bind:value={smtpForm.reply_to}
								/>
							</div>
						</div>
						<label class="mt-3 flex cursor-pointer items-center gap-2">
							<input
								type="checkbox"
								class="checkbox checkbox-sm"
								bind:checked={smtpForm.is_enabled}
							/>
							<span class="text-xs text-base-content/70">
								Enable outgoing SMTP for this account
							</span>
						</label>
					</CollapsibleSection>

					<CollapsibleSection
						title="Archive and retention"
						description="Import older mail into RustShare with optional retention limits"
					>
						{#if $foldersQuery.isError}
							<p class="mb-2 text-xs text-warning" role="alert">
								Folders could not be loaded; archiving is unavailable until the IMAP connection
								works.
							</p>
						{/if}
						<MailArchivePanel
							accountId={selectedMailAccountId}
							folders={$foldersQuery.data ?? []}
						/>
					</CollapsibleSection>

					<CollapsibleSection title="Advanced" description="Folder mapping and removal">
						<div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
							<div class="form-control">
								<label class="label py-1 text-xs font-semibold" for="smtp-sent-folder">
									Sent folder (IMAP)
								</label>
								<input
									id="smtp-sent-folder"
									class="input input-sm input-bordered"
									placeholder="Sent"
									bind:value={smtpForm.sent_folder}
								/>
							</div>
						</div>
						<div class="mt-4 flex flex-wrap gap-2 border-t border-[var(--rs-border)] pt-4">
							{#if selectedSmtp}
								<button
									type="button"
									class="btn btn-sm btn-outline btn-error"
									onclick={handleDeleteSmtp}
								>
									Delete SMTP settings
								</button>
							{/if}
							<button
								type="button"
								class="btn btn-sm btn-error gap-1.5"
								onclick={() => selectedAccount && handleDeleteAccount(selectedAccount.id)}
							>
								<Trash2 size={14} /> Delete this mail account
							</button>
						</div>
					</CollapsibleSection>
				</div>

				<!-- Sticky action footer -->
				<div
					class="sticky bottom-0 flex flex-wrap items-center justify-end gap-2 rounded-b-lg border-t border-[var(--rs-border)] bg-[var(--rs-surface-raised)] px-4 py-3"
				>
					<button
						type="button"
						class="btn btn-sm btn-outline"
						disabled={testingImap || saving}
						onclick={handleTestImap}
					>
						{#if testingImap}<span class="loading loading-spinner loading-xs"></span>{/if}
						Test incoming mail
					</button>
					<button
						type="button"
						class="btn btn-sm btn-outline"
						disabled={!selectedSmtp || testingSmtp || saving}
						title={selectedSmtp ? 'Test the saved SMTP settings' : 'Save SMTP settings first'}
						onclick={handleTestSmtp}
					>
						{#if testingSmtp}<span class="loading loading-spinner loading-xs"></span>{/if}
						Test outgoing mail
					</button>
					<button
						type="button"
						class="btn btn-sm btn-primary"
						disabled={saving}
						onclick={handleSaveAll}
					>
						{#if saving}<span class="loading loading-spinner loading-xs"></span>{/if}
						Save changes
					</button>
				</div>
			{:else}
				<div class="flex flex-col items-center px-6 py-12 text-center">
					<Mail size={28} class="text-base-content/20" />
					<p class="mt-3 text-sm font-medium text-base-content">No mail account configured</p>
					<p class="mt-1 text-xs text-base-content/60">
						Add an IMAP account to use RustShare Mail.
					</p>
					<button type="button" class="btn btn-sm btn-primary mt-4" onclick={openAddAccountForm}>
						Add mail account
					</button>
				</div>
			{/if}
		</div>
	</div>
</div>
