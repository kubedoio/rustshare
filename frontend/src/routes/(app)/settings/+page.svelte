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
	import { listAllFiles } from '$lib/api/files';
	import type { File } from '$lib/api/types';
	import { createQuery } from '$lib/query-compat';
	import {
		FileText,
		Folder,
		FileIcon,
		ImageIcon,
		VideoIcon,
		MusicIcon,
		Clock,
		Smartphone
	} from 'lucide-svelte';

	// Settings components
	import SettingsTabs, { type TabId } from '$lib/settings/SettingsTabs.svelte';
	import SettingsSection from '$lib/settings/SettingsSection.svelte';
	import SettingsRow from '$lib/settings/SettingsRow.svelte';

	// Valid tabs for deep-linking
	const VALID_TABS: TabId[] = [
		'general',
		'security',
		'notifications',
		'devices',
		'appearance',
		'sharing',
		'activity'
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

	// Notifications state (placeholder for future API implementation)
	let emailNotifications = $state(true);
	let fileShareNotifications = $state(true);
	let securityNotifications = $state(true);
	let marketingNotifications = $state(false);

	// Profile state (for email sharing and trash retention)
	let profile = $state<FullUserProfile | null>(null);
	let profileLoading = $state(true);
	let trashRetentionDays = $state<number | null>(30);
	let trashRetentionSaving = $state(false);

	// Sharing defaults (placeholder for future API implementation)
	let defaultLinkExpiration = $state('30');
	let requirePasswordForLinks = $state(false);
	let allowPublicUploads = $state(false);
	let emailSharingEnabled = $state(true);

	// Activity state
	let recentChanges = $state<File[]>([]);
	let activityLoading = $state(true);

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
			const files = await listAllFiles();
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
		{:else if activeTab === 'notifications'}
			<!-- Notifications Tab -->
			<div class="overflow-hidden rounded-xl border border-base-300 bg-base-200">
				<div class="p-6">
					<SettingsSection title="Email Notifications" description="Choose what emails you receive">
						<div class="py-4">
							<label class="flex cursor-pointer items-center justify-between gap-4">
								<div class="flex-1">
									<p class="text-sm font-medium text-base-content">Account and security</p>
									<p class="text-sm text-base-content/60">
										Get notified about sign-ins, password changes, and security alerts
									</p>
								</div>
								<input
									type="checkbox"
									class="toggle toggle-primary"
									bind:checked={securityNotifications}
								/>
							</label>
						</div>
						<div class="border-t border-base-300 py-4">
							<label class="flex cursor-pointer items-center justify-between gap-4">
								<div class="flex-1">
									<p class="text-sm font-medium text-base-content">File sharing</p>
									<p class="text-sm text-base-content/60">
										Get notified when someone shares a file with you
									</p>
								</div>
								<input
									type="checkbox"
									class="toggle toggle-primary"
									bind:checked={fileShareNotifications}
								/>
							</label>
						</div>
						<div class="border-t border-base-300 py-4">
							<label class="flex cursor-pointer items-center justify-between gap-4">
								<div class="flex-1">
									<p class="text-sm font-medium text-base-content">Product updates</p>
									<p class="text-sm text-base-content/60">
										Receive updates about new features and improvements
									</p>
								</div>
								<input
									type="checkbox"
									class="toggle toggle-primary"
									bind:checked={marketingNotifications}
								/>
							</label>
						</div>
					</SettingsSection>

					<div class="mt-6 border-t border-base-300 pt-6">
						<SettingsSection
							title="In-App Notifications"
							description="Notifications shown within RustShare"
						>
							<div class="py-4">
								<label class="flex cursor-pointer items-center justify-between gap-4">
									<div class="flex-1">
										<p class="text-sm font-medium text-base-content">Enable in-app notifications</p>
										<p class="text-sm text-base-content/60">
											Show notification badges and alerts in the application
										</p>
									</div>
									<input
										type="checkbox"
										class="toggle toggle-primary"
										bind:checked={emailNotifications}
									/>
								</label>
							</div>
						</SettingsSection>
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
						title="Default Link Settings"
						description="Preferences for new share links"
					>
						<div class="py-4">
							<label
								class="mb-2 block text-sm font-medium text-base-content"
								for="default-link-expiration">Default link expiration</label
							>
							<select
								id="default-link-expiration"
								class="w-full rounded-lg border border-base-300 bg-base-100 px-4 py-2 text-sm text-base-content focus:border-brand-500/50 focus:outline-hidden sm:w-auto"
								bind:value={defaultLinkExpiration}
							>
								<option value="0">Never</option>
								<option value="7">7 days</option>
								<option value="30">30 days</option>
								<option value="90">90 days</option>
							</select>
						</div>
						<div class="border-t border-base-300 py-4">
							<label class="flex cursor-pointer items-center justify-between gap-4">
								<div class="flex-1">
									<p class="text-sm font-medium text-base-content">Require password for links</p>
									<p class="text-sm text-base-content/60">
										Add password protection to new share links by default
									</p>
								</div>
								<input
									type="checkbox"
									class="toggle toggle-primary"
									bind:checked={requirePasswordForLinks}
								/>
							</label>
						</div>
						<div class="border-t border-base-300 py-4">
							<label class="flex cursor-pointer items-center justify-between gap-4">
								<div class="flex-1">
									<p class="text-sm font-medium text-base-content">Allow public uploads</p>
									<p class="text-sm text-base-content/60">
										Allow upload-only links by default when creating folder shares
									</p>
								</div>
								<input
									type="checkbox"
									class="toggle toggle-primary"
									bind:checked={allowPublicUploads}
								/>
							</label>
						</div>
					</SettingsSection>

					<div class="mt-6 border-t border-base-300 pt-6">
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
		{/if}
	</div>
</div>

{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => (showToast = false)} />
{/if}
