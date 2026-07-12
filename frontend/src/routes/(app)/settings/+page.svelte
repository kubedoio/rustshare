<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { currentUser, authStore } from '$lib/stores/auth';
	import { approveDevicePairing } from '$lib/api/auth';
	import {
		listUserSessions,
		revokeUserSession,
		updateUserPassword,
		listUserDevices,
		revokeUserDevice,
		getProfile,
		updateProfile,
		updateTrashRetention,
		listUserSecurityEvents,
		type UserSession,
		type UserDevice,
		type FullUserProfile,
		type UserSecurityEvent
	} from '$lib/api/users';
	import { themeStore, type Theme } from '$lib/stores/theme';
	import Toast from '$lib/components/common/Toast.svelte';
	import { formatFileSize, formatDate } from '$lib/utils/format';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';
	import { listAllFiles } from '$lib/api/files';
	import type { File } from '$lib/api/types';
	import { getAllModules } from '$lib/modules/registry';
	import { userModulePreferences } from '$lib/stores/userModulePreferences';
	import { createQuery } from '$lib/query-compat';
	import { mailApi, type MailAccount, type MailSmtpSettings } from '$lib/api/mail';
	import {
		FileText,
		Folder,
		FileIcon,
		ImageIcon,
		VideoIcon,
		MusicIcon,
		Clock,
		Smartphone,
		Mail,
		Trash2,
		ShieldCheck,
		AlertCircle,
		Info,
		Settings
	} from 'lucide-svelte';

	// Settings components
	import SettingsTabs, { type TabId } from '$lib/settings/SettingsTabs.svelte';
	import SettingsSection from '$lib/settings/SettingsSection.svelte';
	import SettingsRow from '$lib/settings/SettingsRow.svelte';

	// Valid tabs for deep-linking
	const VALID_TABS: TabId[] = [
		'general',
		'security',
		'devices',
		'appearance',
		'sharing',
		'activity',
		'modules',
		'mail'
	];

	// State
	let activeTab = $state<TabId>('general');
	let showToast = $state(false);
	let toastMessage = $state('');
	let toastType = $state<'success' | 'error' | 'info'>('info');

	// Security state
	let passwordForm = $state({ current_password: '', new_password: '', confirm_password: '' });
	let passwordFormError = $state('');
	let passwordUpdating = $state(false);

	// Sessions state
	let sessions = $state<UserSession[]>([]);
	let sessionsLoading = $state(true);
	let revokingSessionId = $state<string | null>(null);

	// Devices state
	let devices = $state<UserDevice[]>([]);
	let devicesLoading = $state(true);
	let revokingDeviceId = $state<string | null>(null);
	let userCodeInput = $state('');
	let approvingDevice = $state(false);

	// Security events state
	let securityEvents = $state<UserSecurityEvent[]>([]);
	let securityEventsLoading = $state(true);

	// Profile state (for email sharing and trash retention)
	let profile = $state<FullUserProfile | null>(null);
	let profileLoading = $state(true);
	let trashRetentionDays = $state<number | null>(30);
	let trashRetentionSaving = $state(false);

	let emailSharingEnabled = $state(true);

	// Activity state
	let recentChanges = $state<File[]>([]);
	let activityLoading = $state(true);

	// Mail state
	let mailAccounts = $state<MailAccount[]>([]);
	let mailAccountsLoading = $state(false);
	let selectedMailAccountId = $state<string | null>(null);
	let selectedMailAccountSmtp = $state<MailSmtpSettings | null>(null);
	let testingImapId = $state<string | null>(null);
	let testingSmtpId = $state<string | null>(null);

	// IMAP Form
	let imapForm = $state({
		name: '',
		host: '',
		port: 993,
		username: '',
		password: '',
		tls_mode: 'tls' as 'none' | 'starttls' | 'tls'
	});
	let addingMailAccount = $state(false);
	let showAddAccountForm = $state(false);

	// SMTP Form
	let smtpForm = $state({
		host: '',
		port: 587,
		username: '',
		password: '',
		tls_mode: 'tls' as 'none' | 'starttls' | 'tls',
		from_address: '',
		from_name: '',
		reply_to: '',
		sent_folder: '',
		is_enabled: true
	});
	let savingSmtp = $state(false);

	async function loadMailAccounts() {
		mailAccountsLoading = true;
		try {
			mailAccounts = await mailApi.listAccounts();
			if (mailAccounts.length > 0 && !selectedMailAccountId) {
				await selectMailAccount(mailAccounts[0].id);
			}
		} catch (error) {
			console.error('Failed to load mail accounts:', error);
		} finally {
			mailAccountsLoading = false;
		}
	}

	async function selectMailAccount(id: string) {
		selectedMailAccountId = id;
		selectedMailAccountSmtp = null;
		try {
			const smtp = await mailApi.getSmtpSettings(id);
			selectedMailAccountSmtp = smtp;
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
				smtpForm = {
					host: '',
					port: 587,
					username: '',
					password: '',
					tls_mode: 'tls',
					from_address: '',
					from_name: '',
					reply_to: '',
					sent_folder: '',
					is_enabled: true
				};
			}
		} catch (err) {
			console.error('Failed to load SMTP settings:', err);
		}
	}

	async function handleAddMailAccount() {
		addingMailAccount = true;
		try {
			const account = await mailApi.createAccount({
				name: imapForm.name.trim(),
				host: imapForm.host.trim(),
				port: Number(imapForm.port),
				username: imapForm.username.trim(),
				password: imapForm.password,
				tls_mode: imapForm.tls_mode
			});
			showNotification('Mail account added successfully', 'success');
			imapForm = {
				name: '',
				host: '',
				port: 993,
				username: '',
				password: '',
				tls_mode: 'tls'
			};
			showAddAccountForm = false;
			await loadMailAccounts();
			await selectMailAccount(account.id);
		} catch (error) {
			showNotification(error instanceof Error ? error.message : 'Failed to add account', 'error');
		} finally {
			addingMailAccount = false;
		}
	}

	async function handleDeleteMailAccount(id: string) {
		if (
			!confirm(
				'Are you sure you want to disconnect this mail account? Imported mail artifacts will remain.'
			)
		) {
			return;
		}
		try {
			await mailApi.deleteAccount(id);
			showNotification('Mail account disconnected', 'success');
			if (selectedMailAccountId === id) {
				selectedMailAccountId = null;
				selectedMailAccountSmtp = null;
			}
			await loadMailAccounts();
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to delete account',
				'error'
			);
		}
	}

	async function handleTestImap(id: string) {
		testingImapId = id;
		try {
			await mailApi.testAccount(id);
			showNotification('IMAP Connection successful!', 'success');
			await loadMailAccounts();
		} catch (error) {
			showNotification(error instanceof Error ? error.message : 'IMAP Connection failed', 'error');
		} finally {
			testingImapId = null;
		}
	}

	async function handleSaveSmtp() {
		if (!selectedMailAccountId) return;
		savingSmtp = true;
		try {
			const smtp = await mailApi.updateSmtpSettings(selectedMailAccountId, {
				host: smtpForm.host.trim(),
				port: Number(smtpForm.port),
				username: smtpForm.username.trim(),
				password: smtpForm.password ? smtpForm.password : null,
				tls_mode: smtpForm.tls_mode,
				from_address: smtpForm.from_address.trim(),
				from_name: smtpForm.from_name.trim() || null,
				reply_to: smtpForm.reply_to.trim() || null,
				sent_folder: smtpForm.sent_folder.trim() || null,
				is_enabled: smtpForm.is_enabled
			});
			selectedMailAccountSmtp = smtp;
			smtpForm.password = '';
			showNotification('SMTP settings saved successfully', 'success');
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to save SMTP settings',
				'error'
			);
		} finally {
			savingSmtp = false;
		}
	}

	async function handleTestSmtp(id: string) {
		testingSmtpId = id;
		try {
			await mailApi.testSmtpConnection(id);
			showNotification('SMTP Connection successful!', 'success');
		} catch (error) {
			showNotification(error instanceof Error ? error.message : 'SMTP Connection failed', 'error');
		} finally {
			testingSmtpId = null;
		}
	}

	async function handleDeleteSmtp(id: string) {
		if (!confirm('Are you sure you want to delete SMTP settings for this account?')) {
			return;
		}
		try {
			await mailApi.deleteSmtpSettings(id);
			selectedMailAccountSmtp = null;
			smtpForm = {
				host: '',
				port: 587,
				username: '',
				password: '',
				tls_mode: 'tls',
				from_address: '',
				from_name: '',
				reply_to: '',
				sent_folder: '',
				is_enabled: true
			};
			showNotification('SMTP settings deleted', 'success');
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to delete SMTP settings',
				'error'
			);
		}
	}

	$effect(() => {
		if (activeTab === 'mail') {
			loadMailAccounts();
		}
	});

	// Usage data - Consistent with SidebarNav
	let allFilesQuery = $derived(
		createQuery({
			queryKey: ['all-files'],
			queryFn: () => listAllFiles(),
			enabled: !!$currentUser
		})
	);

	let totalSizeUsed = $derived($allFilesQuery.data?.reduce((sum, file) => sum + file.size, 0) || 0);

	let storagePercentage = $derived(
		(() => {
			const quota = $currentUser?.storage_quota;
			if (typeof quota === 'number' && quota > 0) {
				return Math.round((totalSizeUsed / quota) * 100);
			}
			return 0;
		})()
	);

	onMount(() => {
		const tabFromUrl = $page.url.searchParams.get('tab');
		if (tabFromUrl && VALID_TABS.includes(tabFromUrl as TabId)) {
			activeTab = tabFromUrl as TabId;
		}
		void Promise.all([
			loadProfile(),
			refreshSessions(),
			refreshDevices(),
			loadActivity(),
			loadSecurityEvents()
		]);
	});

	async function loadSecurityEvents() {
		securityEventsLoading = true;
		try {
			securityEvents = await listUserSecurityEvents();
		} catch (error) {
			console.error('Failed to load security events:', error);
		} finally {
			securityEventsLoading = false;
		}
	}

	async function loadActivity() {
		activityLoading = true;
		try {
			const files = filterUserVisibleEntries(await listAllFiles());
			// Get last 3 modified items
			recentChanges = [...files]
				.sort((a, b) => new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime())
				.slice(0, 3);
		} catch (error) {
			console.error('Failed to load activity:', error);
		} finally {
			activityLoading = false;
		}
	}

	function getFileIcon(mimeType: string) {
		if (mimeType.startsWith('image/')) return ImageIcon;
		if (mimeType.startsWith('video/')) return VideoIcon;
		if (mimeType.startsWith('audio/')) return MusicIcon;
		if (mimeType.includes('folder')) return Folder;
		return FileText;
	}

	function navigateToFile(file: File) {
		window.location.href = `/files?preview=${file.id}`;
	}

	async function loadProfile() {
		profileLoading = true;
		try {
			profile = await getProfile();
			emailSharingEnabled = profile.email_sharing_enabled;
			trashRetentionDays = profile.trash_retention_days ?? 30;
		} catch (error) {
			console.error('Failed to load profile:', error);
		} finally {
			profileLoading = false;
		}
	}

	async function handleTrashRetentionChange(days: number | null) {
		if (!profile) return;
		trashRetentionSaving = true;
		try {
			await updateTrashRetention({ days });
			showNotification('Trash retention updated', 'success');
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to update trash retention',
				'error'
			);
			// Revert on error
			trashRetentionDays = profile.trash_retention_days ?? 30;
		} finally {
			trashRetentionSaving = false;
		}
	}

	async function handleEmailSharingToggle() {
		if (!profile) return;
		try {
			await updateProfile({
				email_sharing_enabled: emailSharingEnabled
			});
			showNotification('Email sharing preference updated', 'success');
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to update preference',
				'error'
			);
			// Revert on error
			emailSharingEnabled = !emailSharingEnabled;
		}
	}

	async function handlePasswordChange() {
		passwordFormError = '';
		if (
			!passwordForm.current_password ||
			!passwordForm.new_password ||
			!passwordForm.confirm_password
		) {
			passwordFormError = 'All password fields are required';
			return;
		}
		if (passwordForm.new_password !== passwordForm.confirm_password) {
			passwordFormError = 'New password and confirmation do not match';
			return;
		}
		passwordUpdating = true;
		try {
			await updateUserPassword(passwordForm);
			passwordForm = { current_password: '', new_password: '', confirm_password: '' };
			showNotification('Password updated', 'success');
		} catch (error) {
			passwordFormError = error instanceof Error ? error.message : 'Failed to update password';
		} finally {
			passwordUpdating = false;
		}
	}

	async function handleThemeChange(theme: Theme) {
		themeStore.setTheme(theme, true);
		showNotification(`Theme set to ${theme}`, 'success');
	}

	async function refreshDevices() {
		devicesLoading = true;
		try {
			devices = await listUserDevices();
		} catch (error) {
			console.error('Failed to load devices:', error);
		} finally {
			devicesLoading = false;
		}
	}

	async function handleApproveDevice() {
		const code = userCodeInput.replace(/[-\s]/g, '').toUpperCase();
		if (!code || code.length < 8) {
			showNotification('Please enter a valid 8-character pairing code', 'error');
			return;
		}
		approvingDevice = true;
		try {
			const response = await approveDevicePairing(code);
			userCodeInput = '';
			showNotification(`Device "${response.device_name}" approved`, 'success');
			await refreshDevices();
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to approve device',
				'error'
			);
		} finally {
			approvingDevice = false;
		}
	}

	async function handleRevokeDevice(deviceId: string) {
		revokingDeviceId = deviceId;
		try {
			await revokeUserDevice(deviceId);
			await refreshDevices();
			showNotification('Device revoked', 'success');
		} catch (error) {
			showNotification(error instanceof Error ? error.message : 'Failed to revoke device', 'error');
		} finally {
			revokingDeviceId = null;
		}
	}

	async function refreshSessions() {
		sessionsLoading = true;
		try {
			sessions = await listUserSessions();
		} catch (error) {
			console.error('Failed to load sessions:', error);
		} finally {
			sessionsLoading = false;
		}
	}

	async function handleRevokeSession(sessionId: string) {
		revokingSessionId = sessionId;
		try {
			await revokeUserSession(sessionId);
			await refreshSessions();
			showNotification('Session revoked', 'success');
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to revoke session',
				'error'
			);
		} finally {
			revokingSessionId = null;
		}
	}

	function handleLogout() {
		authStore.logout();
	}

	function showNotification(message: string, type: 'success' | 'error' | 'info') {
		toastMessage = message;
		toastType = type;
		showToast = true;
	}

	function formatDateTime(dateString: string | undefined): string {
		if (!dateString) return 'N/A';
		return new Date(dateString).toLocaleString('en-US', {
			year: 'numeric',
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		});
	}
</script>

<svelte:head>
	<title>Settings - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-4xl">
	<!-- Page Header -->
	<div class="mb-6">
		<h1 class="text-2xl font-semibold text-base-content">Settings</h1>
		<p class="mt-1 text-base-content/60">Manage your account, security, and preferences</p>
	</div>

	<!-- Tabs -->
	<SettingsTabs {activeTab} onTabChange={(tab) => (activeTab = tab)} />

	<!-- Tab Content -->
	<div class="mt-6">
		{#if activeTab === 'general'}
			<!-- General Tab -->
			<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200">
				<div class="p-6">
					<!-- Account Summary -->
					<SettingsSection title="Account" description="Your basic account information">
						<div class="flex items-center gap-4 py-4">
							<div
								class="flex h-14 w-14 flex-shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-brand-500 to-brand-600 text-xl font-semibold text-white"
							>
								{$currentUser?.display_name?.charAt(0).toUpperCase() || '?'}
							</div>
							<div class="min-w-0 flex-1">
								<p class="font-semibold text-base-content">{$currentUser?.display_name}</p>
								<p class="text-sm text-base-content/60">{$currentUser?.email}</p>
							</div>
							<a
								href="/profile"
								class="rounded-lg bg-base-300 px-4 py-2 text-sm font-medium text-base-content transition-colors hover:bg-base-300/80"
							>
								Edit Profile
							</a>
						</div>
					</SettingsSection>

					<!-- Storage Section -->
					{#if $currentUser?.storage_quota}
						<div class="mt-6 border-t border-base-300 pt-6">
							<SettingsSection title="Storage" description="Your storage usage">
								<div class="py-4">
									<div class="mb-2 flex items-center justify-between">
										<span class="text-sm text-base-content/60">
											{#if $allFilesQuery.isLoading}
												Calculating usage...
											{:else}
												{formatFileSize(totalSizeUsed)} of {formatFileSize(
													$currentUser.storage_quota ?? 0
												)} used
											{/if}
										</span>
										<span class="text-sm font-medium text-base-content">{storagePercentage}%</span>
									</div>
									<div class="h-2 overflow-hidden rounded-full bg-base-300">
										<div
											class="h-full rounded-full bg-brand-500 transition-all"
											style="width: {Math.min(storagePercentage, 100)}%"
										></div>
									</div>
								</div>
							</SettingsSection>
						</div>
					{/if}

					<!-- Trash Section -->
					<div class="mt-6 border-t border-base-300 pt-6">
						<SettingsSection
							title="Trash"
							description="Automatically delete items after they've been in trash for a set period"
						>
							<div class="py-4">
								<div class="flex items-center justify-between">
									<div>
										<p class="text-sm font-medium text-base-content">Auto-clean trash</p>
										<p class="mt-0.5 text-xs text-base-content/60">
											{#if trashRetentionDays === null}
												Items are kept indefinitely
											{:else}
												Items deleted after {trashRetentionDays} days in trash
											{/if}
										</p>
									</div>
									<select
										class="rounded-lg border border-base-300 bg-base-100 px-3 py-2 text-sm text-base-content focus:border-brand-500/50 focus:outline-hidden"
										value={trashRetentionDays === null ? 'never' : String(trashRetentionDays)}
										onchange={(e) => {
											const val = (e.target as HTMLSelectElement).value;
											const days = val === 'never' ? null : Number(val);
											trashRetentionDays = days;
											handleTrashRetentionChange(days);
										}}
										disabled={trashRetentionSaving}
									>
										<option value="7">7 days</option>
										<option value="30">30 days</option>
										<option value="90">90 days</option>
										<option value="never">Never</option>
									</select>
								</div>
							</div>
						</SettingsSection>
					</div>
				</div>
			</div>
		{:else if activeTab === 'security'}
			<!-- Security Tab -->
			<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200">
				<div class="p-6">
					<SettingsSection title="Password" description="Change your account password">
						{#if passwordFormError}
							<div
								class="mb-4 rounded-lg border border-error/20 bg-error/10 p-3 text-sm text-error"
							>
								{passwordFormError}
							</div>
						{/if}
						<div class="space-y-3 py-4">
							<input
								type="password"
								placeholder="Current password"
								class="w-full rounded-lg border border-base-300 bg-base-100 px-4 py-2 text-sm text-base-content placeholder:text-base-content/40 focus:border-brand-500/50 focus:outline-hidden"
								bind:value={passwordForm.current_password}
							/>
							<input
								type="password"
								placeholder="New password"
								class="w-full rounded-lg border border-base-300 bg-base-100 px-4 py-2 text-sm text-base-content placeholder:text-base-content/40 focus:border-brand-500/50 focus:outline-hidden"
								bind:value={passwordForm.new_password}
							/>
							<input
								type="password"
								placeholder="Confirm new password"
								class="w-full rounded-lg border border-base-300 bg-base-100 px-4 py-2 text-sm text-base-content placeholder:text-base-content/40 focus:border-brand-500/50 focus:outline-hidden"
								bind:value={passwordForm.confirm_password}
							/>
						</div>
						<div class="flex items-center justify-between pt-2">
							<p class="text-xs text-base-content/50">Password must be at least 10 characters</p>
							<button
								type="button"
								class="rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-600 disabled:opacity-50"
								onclick={handlePasswordChange}
								disabled={passwordUpdating}
							>
								{#if passwordUpdating}
									<span
										class="mr-1 inline-block h-4 w-4 animate-spin rounded-full border-2 border-white/30 border-t-white"
									></span>
								{/if}
								Update Password
							</button>
						</div>
					</SettingsSection>

					<div class="mt-6 border-t border-base-300 pt-6">
						<SettingsSection title="Active Sessions" description="Manage your signed-in devices">
							{#if sessionsLoading}
								<div class="py-8 text-center">
									<div
										class="inline-block h-6 w-6 animate-spin rounded-full border-2 border-brand-500 border-t-transparent"
									></div>
								</div>
							{:else if sessions.length === 0}
								<p class="py-4 text-sm text-base-content/60">No active sessions</p>
							{:else}
								<div class="space-y-3 py-4">
									{#each sessions as session}
										<div
											class="flex items-start justify-between rounded-lg border border-base-300 bg-base-100 p-3"
										>
											<div>
												<div class="flex items-center gap-2">
													<p class="text-sm font-medium text-base-content">
														{session.user_agent || 'Browser'}
													</p>
													{#if session.is_current}
														<span
															class="inline-flex items-center rounded bg-success/10 px-2 py-0.5 text-xs font-medium text-success"
															>Current</span
														>
													{/if}
												</div>
												<p class="mt-1 text-xs text-base-content/50">
													{session.ip_address || 'Unknown IP'} • {formatDateTime(
														session.created_at
													)}
												</p>
											</div>
											{#if !session.is_current}
												<button
													type="button"
													class="rounded-lg px-3 py-1.5 text-sm font-medium text-error transition-colors hover:bg-error/10"
													onclick={() => handleRevokeSession(session.id)}
													disabled={revokingSessionId === session.id}
												>
													{#if revokingSessionId === session.id}
														<span
															class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-error/30 border-t-error"
														></span>
													{:else}
														Revoke
													{/if}
												</button>
											{/if}
										</div>
									{/each}
								</div>
							{/if}
						</SettingsSection>
					</div>

					<div class="mt-6 border-t border-base-300 pt-6">
						<SettingsSection
							title="Recent Activity"
							description="Security-relevant events from the last 30 days"
						>
							{#if securityEventsLoading}
								<div class="py-8 text-center">
									<div
										class="inline-block h-6 w-6 animate-spin rounded-full border-2 border-brand-500 border-t-transparent"
									></div>
								</div>
							{:else if securityEvents.length === 0}
								<p class="py-4 text-sm text-base-content/60">No recent activity</p>
							{:else}
								<div class="space-y-2 py-2">
									{#each securityEvents as event}
										<div
											class="flex items-start justify-between rounded-lg border border-base-300 bg-base-100 p-3"
										>
											<div>
												<p class="text-sm font-medium text-base-content">{event.description}</p>
												<p class="mt-0.5 text-xs text-base-content/50">
													{formatDateTime(event.occurred_at)}
													{#if event.ip_address}
														&middot; {event.ip_address}{/if}
												</p>
											</div>
											<span
												class="rounded-full bg-base-200 px-2 py-0.5 text-xs whitespace-nowrap text-base-content/60 capitalize"
											>
												{event.event_type.replace(/_/g, ' ')}
											</span>
										</div>
									{/each}
								</div>
							{/if}
						</SettingsSection>
					</div>

					<div class="mt-6 border-t border-base-300 pt-6">
						<SettingsRow
							label="Sign out"
							description="Sign out of your account on this device"
							actionLabel="Sign out"
							danger={true}
							onAction={handleLogout}
						/>
					</div>
				</div>
			</div>
		{:else if activeTab === 'devices'}
			<!-- Devices Tab -->
			<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200">
				<div class="p-6">
					<SettingsSection
						title="Pair New Device"
						description="Connect a mobile device to your account"
					>
						<div class="py-4">
							<p class="mb-3 text-sm text-base-content/70">
								Enter the 8-character pairing code from your device
							</p>
							<div class="flex gap-3">
								<input
									type="text"
									placeholder="XXXX-XXXX"
									class="w-32 rounded-lg border border-base-300 bg-base-100 px-4 py-2 font-mono text-sm text-base-content uppercase placeholder:text-base-content/40 focus:border-brand-500/50 focus:outline-hidden"
									bind:value={userCodeInput}
									maxlength="9"
									oninput={(e) =>
										(userCodeInput =
											userCodeInput
												.replace(/[^a-zA-Z0-9]/g, '')
												.toUpperCase()
												.slice(0, 4) +
											(userCodeInput.length > 4 ? '-' + userCodeInput.slice(4, 8) : ''))}
								/>
								<button
									type="button"
									class="rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-600 disabled:opacity-50"
									onclick={handleApproveDevice}
									disabled={approvingDevice || !userCodeInput}
								>
									{#if approvingDevice}
										<span
											class="mr-1 inline-block h-4 w-4 animate-spin rounded-full border-2 border-white/30 border-t-white"
										></span>
									{/if}
									Approve
								</button>
							</div>
						</div>
					</SettingsSection>

					<div class="mt-6 border-t border-base-300 pt-6">
						<SettingsSection
							title="Connected Devices"
							description="Devices that have access to your account"
						>
							{#if devicesLoading}
								<div class="py-8 text-center">
									<div
										class="inline-block h-6 w-6 animate-spin rounded-full border-2 border-brand-500 border-t-transparent"
									></div>
								</div>
							{:else if devices.length === 0}
								<div class="py-6 text-center">
									<Smartphone size={32} class="mx-auto mb-2 text-base-content/20" />
									<p class="text-sm text-base-content/60">No devices connected</p>
									<p class="mt-1 text-xs text-base-content/40">
										Use the <a href="/device" class="text-brand-500 hover:underline"
											>Pair This Device</a
										> link below to connect a desktop or mobile client.
									</p>
								</div>
							{:else}
								<div class="space-y-3 py-4">
									{#each devices as device}
										<div
											class="flex items-center justify-between rounded-lg border border-base-300 bg-base-100 p-3"
										>
											<div>
												<p class="text-sm font-medium text-base-content">{device.device_name}</p>
												<p class="text-xs text-base-content/50">
													Last active: {device.last_used_at
														? formatDateTime(device.last_used_at)
														: 'Never'}
												</p>
											</div>
											<button
												type="button"
												class="rounded-lg px-3 py-1.5 text-sm font-medium text-error transition-colors hover:bg-error/10"
												onclick={() => handleRevokeDevice(device.id)}
												disabled={revokingDeviceId === device.id}
											>
												{#if revokingDeviceId === device.id}
													<span
														class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-error/30 border-t-error"
													></span>
												{:else}
													Revoke
												{/if}
											</button>
										</div>
									{/each}
								</div>
							{/if}
						</SettingsSection>
					</div>

					<div class="mt-6 border-t border-base-300 pt-6">
						<SettingsRow
							label="This Device"
							description="Pair this browser for mobile access"
							actionLabel="Pair This Device"
							actionHref="/device"
						/>
					</div>
				</div>
			</div>
		{:else if activeTab === 'appearance'}
			<!-- Appearance Tab -->
			<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200">
				<div class="p-6">
					<SettingsSection title="Theme" description="Choose how RustShare looks">
						<div class="grid grid-cols-1 gap-4 py-4 sm:grid-cols-3">
							<button
								type="button"
								class="rounded-xl border-2 p-4 text-left transition-all
									{$themeStore === 'light'
									? 'border-brand-500 bg-brand-500/10'
									: 'hover:border-base-400 border-base-300'}"
								onclick={() => handleThemeChange('light')}
							>
								<div
									class="mb-3 flex h-10 w-10 items-center justify-center rounded-lg border border-base-300 bg-base-100"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										class="h-5 w-5 text-yellow-500"
									>
										<circle cx="12" cy="12" r="5" />
										<path
											d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"
										/>
									</svg>
								</div>
								<p class="font-medium text-base-content">Light</p>
								<p class="mt-1 text-xs text-base-content/60">Always use light mode</p>
							</button>

							<button
								type="button"
								class="rounded-xl border-2 p-4 text-left transition-all
									{$themeStore === 'dark'
									? 'border-brand-500 bg-brand-500/10'
									: 'hover:border-base-400 border-base-300'}"
								onclick={() => handleThemeChange('dark')}
							>
								<div
									class="border-base-400 mb-3 flex h-10 w-10 items-center justify-center rounded-lg border bg-base-300"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										class="h-5 w-5 text-blue-400"
									>
										<path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
									</svg>
								</div>
								<p class="font-medium text-base-content">Dark</p>
								<p class="mt-1 text-xs text-base-content/60">Always use dark mode</p>
							</button>

							<button
								type="button"
								class="rounded-xl border-2 p-4 text-left transition-all
									{$themeStore === 'system'
									? 'border-brand-500 bg-brand-500/10'
									: 'hover:border-base-400 border-base-300'}"
								onclick={() => handleThemeChange('system')}
							>
								<div
									class="mb-3 flex h-10 w-10 items-center justify-center rounded-lg border border-base-300 bg-gradient-to-br from-base-100 to-base-300"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										class="h-5 w-5 text-base-content"
									>
										<rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
										<line x1="8" y1="21" x2="16" y2="21" />
										<line x1="12" y1="17" x2="12" y2="21" />
									</svg>
								</div>
								<p class="font-medium text-base-content">System</p>
								<p class="mt-1 text-xs text-base-content/60">Follow system preference</p>
							</button>
						</div>
					</SettingsSection>
				</div>
			</div>
		{:else if activeTab === 'sharing'}
			<!-- Sharing Tab -->
			<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200">
				<div class="p-6">
					<SettingsSection
						title="Email Sharing"
						description="Control how others can share with you"
					>
						{#if profileLoading}
							<div class="py-4 text-center">
								<div
									class="inline-block h-5 w-5 animate-spin rounded-full border-2 border-brand-500 border-t-transparent"
								></div>
							</div>
						{:else}
							<div class="py-4">
								<label class="flex cursor-pointer items-center justify-between gap-4">
									<div class="flex-1">
										<p class="text-sm font-medium text-base-content">Allow email sharing</p>
										<p class="text-sm text-base-content/60">
											Allow other users to see your email when sharing files
										</p>
									</div>
									<input
										type="checkbox"
										class="toggle toggle-primary"
										bind:checked={emailSharingEnabled}
										onchange={handleEmailSharingToggle}
									/>
								</label>
							</div>
						{/if}
					</SettingsSection>
				</div>
			</div>
		{:else if activeTab === 'activity'}
			<!-- Activity Tab -->
			<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200">
				<div class="p-6">
					<SettingsSection
						title="Recent Changes"
						description="Last 3 modified items in your workspace"
					>
						{#if activityLoading}
							<div class="py-8 text-center">
								<div
									class="inline-block h-6 w-6 animate-spin rounded-full border-2 border-brand-500 border-t-transparent"
								></div>
							</div>
						{:else if recentChanges.length === 0}
							<div class="py-8 text-center">
								<Clock size={32} class="mx-auto mb-3 text-base-content/20" />
								<p class="text-sm text-base-content/60">No recent changes found</p>
							</div>
						{:else}
							<div class="py-2">
								{#each recentChanges as file, index}
									{@const Icon = getFileIcon(file.mime_type)}
									<button
										onclick={() => navigateToFile(file)}
										class="flex w-full items-center gap-3 rounded-xl p-3 text-left transition-colors hover:bg-base-100"
									>
										<div
											class="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-lg border border-base-300 bg-base-100"
										>
											<Icon size={16} class="text-brand-500" />
										</div>
										<div class="min-w-0 flex-1">
											<p class="truncate text-sm font-medium text-base-content">{file.name}</p>
											<p class="flex items-center gap-1 text-xs text-base-content/50">
												<Folder size={10} />
												<span class="truncate">{file.path || 'Root'}</span>
											</p>
										</div>
										<div class="flex-shrink-0 text-right">
											<p class="text-xs text-base-content/60">{formatDate(file.modified_at)}</p>
											<p class="text-[10px] tracking-wider text-base-content/40 uppercase">
												{formatFileSize(file.size)}
											</p>
										</div>
									</button>
									{#if index < recentChanges.length - 1}
										<div class="mx-3 h-px bg-base-300/50"></div>
									{/if}
								{/each}
							</div>
						{/if}
					</SettingsSection>

					<div class="mt-6 border-t border-base-300 pt-6">
						<p class="text-center text-xs text-base-content/50">
							This shows a lightweight audit snapshot. For detailed file history, visit your <a
								href="/files"
								class="text-brand-500 hover:underline">Library</a
							>.
						</p>
					</div>
				</div>
			</div>
		{:else if activeTab === 'modules'}
			<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200">
				<div class="p-6">
					<SettingsSection
						title="Module Preferences"
						description="Enable or disable modules you want to use in your workspace."
					>
						<div class="flex flex-col gap-4">
							{#each getAllModules().filter((m) => m.enabled) as module}
								<div
									class="flex items-center justify-between rounded-xl border border-base-300 bg-base-100 p-4"
								>
									<div class="flex items-center gap-3">
										<div
											class="flex h-9 w-9 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
										>
											<FileText size={18} />
										</div>
										<div>
											<p class="text-sm font-medium text-base-content">{module.displayName}</p>
											<p class="text-xs text-base-content/50">{module.description}</p>
										</div>
									</div>
									<label class="relative inline-flex cursor-pointer items-center">
										<input
											type="checkbox"
											class="peer sr-only"
											checked={$userModulePreferences.preferences[module.key] !== false}
											onchange={(e) =>
												userModulePreferences.toggle(module.key, e.currentTarget.checked)}
										/>
										<div
											class="peer h-6 w-11 rounded-full bg-base-300 peer-checked:bg-brand-500 after:absolute after:start-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:bg-white after:transition-all after:content-[''] peer-checked:after:translate-x-full"
										></div>
									</label>
								</div>
							{/each}
						</div>
					</SettingsSection>
				</div>
			</div>
		{:else if activeTab === 'mail'}
			<!-- Mail Accounts Tab -->
			<div class="grid grid-cols-1 gap-6 md:grid-cols-[280px_1fr]">
				<!-- Left Column: Accounts List -->
				<div class="flex flex-col gap-4 rounded-xl border border-base-300 bg-base-200 p-4 h-fit">
					<div class="flex items-center justify-between">
						<h2 class="text-sm font-semibold text-base-content">Accounts</h2>
						<button
							type="button"
							class="btn btn-xs btn-primary"
							onclick={() => {
								showAddAccountForm = true;
								selectedMailAccountId = null;
							}}
						>
							+ Add
						</button>
					</div>

					{#if mailAccountsLoading}
						<div class="py-4 text-center">
							<span class="loading loading-spinner loading-sm text-brand-500"></span>
						</div>
					{:else if mailAccounts.length === 0}
						<p class="py-4 text-center text-xs text-base-content/50">No accounts configured.</p>
					{:else}
						<div class="flex flex-col gap-2">
							{#each mailAccounts as account}
								<button
									type="button"
									class="w-full text-left rounded-lg p-3 border transition-all text-xs
										{selectedMailAccountId === account.id
										? 'border-brand-500 bg-brand-500/10 text-base-content'
										: 'border-base-300 bg-base-100 text-base-content/70 hover:bg-base-200'}"
									onclick={() => {
										showAddAccountForm = false;
										selectMailAccount(account.id);
									}}
								>
									<p class="font-bold truncate">{account.name}</p>
									<p class="text-[10px] text-base-content/50 truncate mt-0.5">{account.username}</p>
									<p class="text-[10px] text-base-content/50 truncate">
										{account.host}:{account.port}
									</p>
									{#if account.last_error}
										<p class="text-[10px] text-error truncate mt-1">{account.last_error}</p>
									{/if}
								</button>
							{/each}
						</div>
					{/if}
				</div>

				<!-- Right Column: Detail / Add form -->
				<div class="flex flex-col gap-6">
					{#if showAddAccountForm}
						<!-- Add Account Form -->
						<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200 p-6">
							<SettingsSection
								title="Add Mail Account"
								description="Configure a new IMAP mail account"
							>
								<form
									class="flex flex-col gap-4 py-4"
									onsubmit={(event) => {
										event.preventDefault();
										handleAddMailAccount();
									}}
								>
									<div class="grid grid-cols-1 gap-4 md:grid-cols-2">
										<div class="form-control">
											<label class="label text-xs font-semibold" for="imap-name">Account Name</label
											>
											<input
												id="imap-name"
												class="input input-sm input-bordered bg-base-100"
												placeholder="My Email"
												bind:value={imapForm.name}
												required
											/>
										</div>
										<div class="form-control">
											<label class="label text-xs font-semibold" for="imap-username"
												>Username / Email Address</label
											>
											<input
												id="imap-username"
												class="input input-sm input-bordered bg-base-100"
												placeholder="user@example.com"
												bind:value={imapForm.username}
												required
											/>
										</div>
									</div>

									<div class="grid grid-cols-1 gap-4 md:grid-cols-3">
										<div class="form-control col-span-2">
											<label class="label text-xs font-semibold" for="imap-host">IMAP Host</label>
											<input
												id="imap-host"
												class="input input-sm input-bordered bg-base-100"
												placeholder="imap.example.com"
												bind:value={imapForm.host}
												required
											/>
										</div>
										<div class="form-control">
											<label class="label text-xs font-semibold" for="imap-port">Port</label>
											<input
												id="imap-port"
												type="number"
												class="input input-sm input-bordered bg-base-100"
												bind:value={imapForm.port}
												required
											/>
										</div>
									</div>

									<div class="grid grid-cols-1 gap-4 md:grid-cols-2">
										<div class="form-control">
											<label class="label text-xs font-semibold" for="imap-password"
												>Password / App Token</label
											>
											<input
												id="imap-password"
												type="password"
												class="input input-sm input-bordered bg-base-100"
												placeholder="••••••••"
												bind:value={imapForm.password}
												required
											/>
										</div>
										<div class="form-control">
											<label class="label text-xs font-semibold" for="imap-tls">Security Mode</label
											>
											<select
												id="imap-tls"
												class="select select-sm select-bordered bg-base-100"
												bind:value={imapForm.tls_mode}
											>
												<option value="tls">TLS (SSL)</option>
												<option value="starttls">STARTTLS</option>
												<option value="none">Plain / None</option>
											</select>
										</div>
									</div>

									<div class="flex justify-end gap-2 mt-2">
										<button
											type="button"
											class="btn btn-sm btn-outline"
											onclick={() => (showAddAccountForm = false)}
										>
											Cancel
										</button>
										<button
											type="submit"
											class="btn btn-sm btn-primary"
											disabled={addingMailAccount}
										>
											{#if addingMailAccount}
												<span class="loading loading-spinner loading-xs"></span>
											{/if}
											Save Account
										</button>
									</div>
								</form>
							</SettingsSection>
						</div>
					{:else if selectedMailAccountId}
						<!-- Account Management & SMTP -->
						{@const account = mailAccounts.find((a) => a.id === selectedMailAccountId)}
						{#if account}
							<!-- IMAP Detail and Test -->
							<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200 p-6">
								<div class="flex items-center justify-between border-b border-base-300 pb-3 mb-4">
									<div>
										<h2 class="text-md font-semibold text-base-content">{account.name}</h2>
										<p class="text-xs text-base-content/60">IMAP account details</p>
									</div>
									<div class="flex gap-2">
										<button
											type="button"
											class="btn btn-sm btn-outline text-xs"
											onclick={() => handleTestImap(account.id)}
											disabled={testingImapId === account.id}
										>
											{#if testingImapId === account.id}
												<span class="loading loading-spinner loading-xs mr-1"></span>
											{/if}
											Test IMAP
										</button>
										<button
											type="button"
											class="btn btn-sm btn-error btn-outline"
											onclick={() => handleDeleteMailAccount(account.id)}
										>
											<Trash2 size={14} />
										</button>
									</div>
								</div>

								<div class="grid grid-cols-2 gap-4 text-xs">
									<div>
										<span class="text-base-content/50">IMAP Host:</span>
										<span class="ml-1 font-medium"
											>{account.host}:{account.port} ({account.tls_mode})</span
										>
									</div>
									<div>
										<span class="text-base-content/50">Username:</span>
										<span class="ml-1 font-medium">{account.username}</span>
									</div>
								</div>
							</div>

							<!-- SMTP Form Section -->
							<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200 p-6">
								<SettingsSection
									title="Outgoing SMTP Configuration"
									description="Set up SMTP configuration to allow composing and replying to mail"
								>
									<form
										class="flex flex-col gap-4 py-4"
										onsubmit={(event) => {
											event.preventDefault();
											handleSaveSmtp();
										}}
									>
										<div class="grid grid-cols-1 gap-4 md:grid-cols-3">
											<div class="form-control col-span-2">
												<label class="label text-xs font-semibold" for="smtp-host">SMTP Host</label>
												<input
													id="smtp-host"
													class="input input-sm input-bordered bg-base-100"
													placeholder="smtp.example.com"
													bind:value={smtpForm.host}
													required
												/>
											</div>
											<div class="form-control">
												<label class="label text-xs font-semibold" for="smtp-port">SMTP Port</label>
												<input
													id="smtp-port"
													type="number"
													class="input input-sm input-bordered bg-base-100"
													bind:value={smtpForm.port}
													required
												/>
											</div>
										</div>

										<div class="grid grid-cols-1 gap-4 md:grid-cols-2">
											<div class="form-control">
												<label class="label text-xs font-semibold" for="smtp-username"
													>SMTP Username</label
												>
												<input
													id="smtp-username"
													class="input input-sm input-bordered bg-base-100"
													placeholder="user@example.com"
													bind:value={smtpForm.username}
													required
												/>
											</div>
											<div class="form-control">
												<label class="label text-xs font-semibold" for="smtp-tls"
													>Security Mode</label
												>
												<select
													id="smtp-tls"
													class="select select-sm select-bordered bg-base-100"
													bind:value={smtpForm.tls_mode}
												>
													<option value="tls">TLS</option>
													<option value="starttls">STARTTLS</option>
													<option value="none">Plain / None</option>
												</select>
											</div>
										</div>

										<div class="form-control">
											<label class="label text-xs font-semibold" for="smtp-password">
												SMTP Password / Token
												{#if selectedMailAccountSmtp}
													<span class="text-base-content/40 font-normal ml-1"
														>(Leave blank to keep unchanged)</span
													>
												{/if}
											</label>
											<input
												id="smtp-password"
												type="password"
												class="input input-sm input-bordered bg-base-100"
												placeholder="••••••••"
												bind:value={smtpForm.password}
												required={!selectedMailAccountSmtp}
											/>
										</div>

										<div class="grid grid-cols-1 gap-4 md:grid-cols-2">
											<div class="form-control">
												<label class="label text-xs font-semibold" for="smtp-from"
													>From Address</label
												>
												<input
													id="smtp-from"
													type="email"
													class="input input-sm input-bordered bg-base-100"
													placeholder="me@example.com"
													bind:value={smtpForm.from_address}
													required
												/>
											</div>
											<div class="form-control">
												<label class="label text-xs font-semibold" for="smtp-from-name"
													>From Name</label
												>
												<input
													id="smtp-from-name"
													class="input input-sm input-bordered bg-base-100"
													placeholder="My Display Name"
													bind:value={smtpForm.from_name}
												/>
											</div>
										</div>

										<div class="grid grid-cols-1 gap-4 md:grid-cols-2">
											<div class="form-control">
												<label class="label text-xs font-semibold" for="smtp-replyto"
													>Reply-To Address (Optional)</label
												>
												<input
													id="smtp-replyto"
													type="email"
													class="input input-sm input-bordered bg-base-100"
													placeholder="reply@example.com"
													bind:value={smtpForm.reply_to}
												/>
											</div>
											<div class="form-control">
												<label class="label text-xs font-semibold" for="smtp-sent-folder"
													>Sent Folder Name (Optional)</label
												>
												<input
													id="smtp-sent-folder"
													class="input input-sm input-bordered bg-base-100"
													placeholder="Sent"
													bind:value={smtpForm.sent_folder}
												/>
											</div>
										</div>

										<label class="label cursor-pointer justify-start gap-2 py-1">
											<input
												type="checkbox"
												class="checkbox checkbox-xs"
												bind:checked={smtpForm.is_enabled}
											/>
											<span class="label-text text-xs">Enable Outgoing SMTP for this account</span>
										</label>

										<div class="flex justify-end gap-2 mt-4 border-t border-base-300 pt-3">
											{#if selectedMailAccountSmtp}
												<button
													type="button"
													class="btn btn-sm btn-error btn-outline"
													onclick={() => handleDeleteSmtp(selectedMailAccountId!)}
												>
													Delete SMTP
												</button>
												<button
													type="button"
													class="btn btn-sm btn-outline"
													onclick={() => handleTestSmtp(selectedMailAccountId!)}
													disabled={testingSmtpId === selectedMailAccountId}
												>
													{#if testingSmtpId === selectedMailAccountId}
														<span class="loading loading-spinner loading-xs mr-1"></span>
													{/if}
													Test Connection
												</button>
											{/if}
											<button type="submit" class="btn btn-sm btn-primary" disabled={savingSmtp}>
												{#if savingSmtp}
													<span class="loading loading-spinner loading-xs mr-1"></span>
												{/if}
												Save SMTP Settings
											</button>
										</div>
									</form>
								</SettingsSection>
							</div>
						{/if}
					{:else}
						<!-- Empty Detail state -->
						<div
							class="overflow-hidden rounded-xl border border-base-300 bg-base-200 p-8 text-center"
						>
							<Mail size={32} class="mx-auto mb-2 text-base-content/20" />
							<p class="text-sm font-medium text-base-content">No account selected</p>
							<p class="text-xs text-base-content/60 mt-1">
								Select an existing account from the list or add a new one.
							</p>
							<button
								type="button"
								class="btn btn-sm btn-primary mt-4"
								onclick={() => (showAddAccountForm = true)}
							>
								Add Mail Account
							</button>
						</div>
					{/if}
				</div>
			</div>
		{/if}
	</div>
</div>

{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => (showToast = false)} />
{/if}
