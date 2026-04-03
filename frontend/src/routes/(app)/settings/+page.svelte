<script lang="ts">
	import { onMount } from 'svelte';
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
		type UserSession,
		type UserDevice,
		type FullUserProfile
	} from '$lib/api/users';
	import { themeStore, type Theme } from '$lib/stores/theme';
	import Toast from '$lib/components/common/Toast.svelte';
	import { formatFileSize, formatDate } from '$lib/utils/format';
	import { listAllFiles } from '$lib/api/files';
	import type { File } from '$lib/api/types';
	import { FileText, Folder, FileIcon, ImageIcon, VideoIcon, MusicIcon, Clock } from 'lucide-svelte';
	
	// Settings components
	import SettingsTabs, { type TabId } from '$lib/settings/SettingsTabs.svelte';
	import SettingsSection from '$lib/settings/SettingsSection.svelte';
	import SettingsRow from '$lib/settings/SettingsRow.svelte';

	// State
	let activeTab: TabId = 'general';
	let showToast = false;
	let toastMessage = '';
	let toastType: 'success' | 'error' | 'info' = 'info';
	
	// Security state
	let passwordForm = { current_password: '', new_password: '', confirm_password: '' };
	let passwordFormError = '';
	let passwordUpdating = false;
	
	// Sessions state
	let sessions: UserSession[] = [];
	let sessionsLoading = true;
	let revokingSessionId: string | null = null;
	
	// Devices state
	let devices: UserDevice[] = [];
	let devicesLoading = true;
	let revokingDeviceId: string | null = null;
	let userCodeInput = '';
	let approvingDevice = false;
	
	// Notifications state (placeholder for future API implementation)
	let emailNotifications = true;
	let fileShareNotifications = true;
	let securityNotifications = true;
	let marketingNotifications = false;
	
	// Profile state (for email sharing toggle)
	let profile: FullUserProfile | null = null;
	let profileLoading = true;
	
	// Sharing defaults (placeholder for future API implementation)
	let defaultLinkExpiration = '30';
	let requirePasswordForLinks = false;
	let allowPublicUploads = false;
	let emailSharingEnabled = true;

	// Activity state
	let recentChanges: File[] = [];
	let activityLoading = true;

	$: storagePercentage =
		$currentUser?.storage_quota && Number.isFinite($currentUser?.storage_used) && $currentUser.storage_used >= 0
			? Math.round(($currentUser.storage_used / $currentUser.storage_quota) * 100)
			: 0;

	onMount(async () => {
		await Promise.all([
			loadProfile(),
			refreshSessions(),
			refreshDevices(),
			loadActivity()
		]);
	});

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
		} catch (error) {
			console.error('Failed to load profile:', error);
		} finally {
			profileLoading = false;
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
			showNotification(error instanceof Error ? error.message : 'Failed to update preference', 'error');
			// Revert on error
			emailSharingEnabled = !emailSharingEnabled;
		}
	}

	async function handlePasswordChange() {
		passwordFormError = '';
		if (!passwordForm.current_password || !passwordForm.new_password || !passwordForm.confirm_password) {
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
			showNotification(error instanceof Error ? error.message : 'Failed to approve device', 'error');
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
			showNotification(error instanceof Error ? error.message : 'Failed to revoke session', 'error');
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

<div class="max-w-4xl mx-auto">
	<!-- Page Header -->
	<div class="mb-6">
		<h1 class="text-2xl font-semibold text-base-content">Settings</h1>
		<p class="text-base-content/60 mt-1">Manage your account, security, and preferences</p>
	</div>

	<!-- Tabs -->
	<SettingsTabs {activeTab} onTabChange={(tab) => activeTab = tab} />

	<!-- Tab Content -->
	<div class="mt-6">
		{#if activeTab === 'general'}
			<!-- General Tab -->
			<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
				<div class="p-6">
					<!-- Account Summary -->
					<SettingsSection title="Account" description="Your basic account information">
						<div class="flex items-center gap-4 py-4">
							<div class="w-14 h-14 rounded-xl bg-gradient-to-br from-brand-500 to-brand-600 flex items-center justify-center text-white text-xl font-semibold flex-shrink-0">
								{$currentUser?.display_name?.charAt(0).toUpperCase() || '?'}
							</div>
							<div class="flex-1 min-w-0">
								<p class="font-semibold text-base-content">{$currentUser?.display_name}</p>
								<p class="text-sm text-base-content/60">{$currentUser?.email}</p>
							</div>
							<a href="/profile" class="px-4 py-2 text-sm font-medium bg-base-300 hover:bg-base-300/80 text-base-content rounded-lg transition-colors">
								Edit Profile
							</a>
						</div>
					</SettingsSection>

					<!-- Storage Section -->
					{#if $currentUser?.storage_quota}
						<div class="border-t border-base-300 pt-6 mt-6">
							<SettingsSection title="Storage" description="Your storage usage">
								<div class="py-4">
									<div class="flex items-center justify-between mb-2">
										<span class="text-sm text-base-content/60">{formatFileSize($currentUser.storage_used ?? 0)} of {formatFileSize($currentUser.storage_quota ?? 0)} used</span>
										<span class="text-sm font-medium text-base-content">{storagePercentage}%</span>
									</div>
									<div class="h-2 bg-base-300 rounded-full overflow-hidden">
										<div class="h-full bg-brand-500 rounded-full transition-all" style="width: {Math.min(storagePercentage, 100)}%"></div>
									</div>
								</div>
							</SettingsSection>
						</div>
					{/if}
				</div>
			</div>

		{:else if activeTab === 'security'}
			<!-- Security Tab -->
			<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
				<div class="p-6">
					<SettingsSection title="Password" description="Change your account password">
						{#if passwordFormError}
							<div class="mb-4 p-3 bg-error/10 border border-error/20 rounded-lg text-sm text-error">
								{passwordFormError}
							</div>
						{/if}
						<div class="space-y-3 py-4">
							<input
								type="password"
								placeholder="Current password"
								class="w-full px-4 py-2 bg-base-100 border border-base-300 rounded-lg text-sm text-base-content placeholder:text-base-content/40 focus:outline-none focus:border-brand-500/50"
								bind:value={passwordForm.current_password}
							/>
							<input
								type="password"
								placeholder="New password"
								class="w-full px-4 py-2 bg-base-100 border border-base-300 rounded-lg text-sm text-base-content placeholder:text-base-content/40 focus:outline-none focus:border-brand-500/50"
								bind:value={passwordForm.new_password}
							/>
							<input
								type="password"
								placeholder="Confirm new password"
								class="w-full px-4 py-2 bg-base-100 border border-base-300 rounded-lg text-sm text-base-content placeholder:text-base-content/40 focus:outline-none focus:border-brand-500/50"
								bind:value={passwordForm.confirm_password}
							/>
						</div>
						<div class="flex items-center justify-between pt-2">
							<p class="text-xs text-base-content/50">Password must be at least 10 characters</p>
							<button
								type="button"
								class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors disabled:opacity-50"
								on:click={handlePasswordChange}
								disabled={passwordUpdating}
							>
								{#if passwordUpdating}
									<span class="inline-block w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin mr-1"></span>
								{/if}
								Update Password
							</button>
						</div>
					</SettingsSection>

					<div class="border-t border-base-300 pt-6 mt-6">
						<SettingsSection title="Active Sessions" description="Manage your signed-in devices">
							{#if sessionsLoading}
								<div class="py-8 text-center">
									<div class="inline-block w-6 h-6 border-2 border-brand-500 border-t-transparent rounded-full animate-spin"></div>
								</div>
							{:else if sessions.length === 0}
								<p class="py-4 text-sm text-base-content/60">No active sessions</p>
							{:else}
								<div class="space-y-3 py-4">
									{#each sessions as session}
										<div class="flex items-start justify-between p-3 bg-base-100 rounded-lg border border-base-300">
											<div>
												<div class="flex items-center gap-2">
													<p class="text-sm font-medium text-base-content">{session.user_agent || 'Browser'}</p>
													{#if session.is_current}
														<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-success/10 text-success">Current</span>
													{/if}
												</div>
												<p class="text-xs text-base-content/50 mt-1">{session.ip_address || 'Unknown IP'} • {formatDateTime(session.created_at)}</p>
											</div>
											{#if !session.is_current}
												<button
													type="button"
													class="px-3 py-1.5 text-sm font-medium text-error hover:bg-error/10 rounded-lg transition-colors"
													on:click={() => handleRevokeSession(session.id)}
													disabled={revokingSessionId === session.id}
												>
													{#if revokingSessionId === session.id}
														<span class="inline-block w-4 h-4 border-2 border-error/30 border-t-error rounded-full animate-spin"></span>
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

					<div class="border-t border-base-300 pt-6 mt-6">
						<SettingsRow
							label="Sign out"
							description="Sign out of your account on this device"
							actionLabel="Sign out"
							danger={true}
							on:action={handleLogout}
						/>
					</div>
				</div>
			</div>

		{:else if activeTab === 'notifications'}
			<!-- Notifications Tab -->
			<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
				<div class="p-6">
					<SettingsSection title="Email Notifications" description="Choose what emails you receive">
						<div class="py-4">
							<label class="flex items-center justify-between gap-4 cursor-pointer">
								<div class="flex-1">
									<p class="text-sm font-medium text-base-content">Account and security</p>
									<p class="text-sm text-base-content/60">Get notified about sign-ins, password changes, and security alerts</p>
								</div>
								<input type="checkbox" class="toggle toggle-primary" bind:checked={securityNotifications} />
							</label>
						</div>
						<div class="py-4 border-t border-base-300">
							<label class="flex items-center justify-between gap-4 cursor-pointer">
								<div class="flex-1">
									<p class="text-sm font-medium text-base-content">File sharing</p>
									<p class="text-sm text-base-content/60">Get notified when someone shares a file with you</p>
								</div>
								<input type="checkbox" class="toggle toggle-primary" bind:checked={fileShareNotifications} />
							</label>
						</div>
						<div class="py-4 border-t border-base-300">
							<label class="flex items-center justify-between gap-4 cursor-pointer">
								<div class="flex-1">
									<p class="text-sm font-medium text-base-content">Product updates</p>
									<p class="text-sm text-base-content/60">Receive updates about new features and improvements</p>
								</div>
								<input type="checkbox" class="toggle toggle-primary" bind:checked={marketingNotifications} />
							</label>
						</div>
					</SettingsSection>

					<div class="border-t border-base-300 pt-6 mt-6">
						<SettingsSection title="In-App Notifications" description="Notifications shown within RustShare">
							<div class="py-4">
								<label class="flex items-center justify-between gap-4 cursor-pointer">
									<div class="flex-1">
										<p class="text-sm font-medium text-base-content">Enable in-app notifications</p>
										<p class="text-sm text-base-content/60">Show notification badges and alerts in the application</p>
									</div>
									<input type="checkbox" class="toggle toggle-primary" bind:checked={emailNotifications} />
								</label>
							</div>
						</SettingsSection>
					</div>
				</div>
			</div>

		{:else if activeTab === 'devices'}
			<!-- Devices Tab -->
			<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
				<div class="p-6">
					<SettingsSection title="Pair New Device" description="Connect a mobile device to your account">
						<div class="py-4">
							<p class="text-sm text-base-content/70 mb-3">Enter the 8-character pairing code from your device</p>
							<div class="flex gap-3">
								<input
									type="text"
									placeholder="XXXX-XXXX"
									class="px-4 py-2 bg-base-100 border border-base-300 rounded-lg text-sm font-mono text-base-content placeholder:text-base-content/40 focus:outline-none focus:border-brand-500/50 w-32 uppercase"
									bind:value={userCodeInput}
									maxlength="9"
									on:input={(e) => userCodeInput = userCodeInput.replace(/[^a-zA-Z0-9]/g, '').toUpperCase().slice(0, 4) + (userCodeInput.length > 4 ? '-' + userCodeInput.slice(4, 8) : '')}
								/>
								<button
									type="button"
									class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors disabled:opacity-50"
									on:click={handleApproveDevice}
									disabled={approvingDevice || !userCodeInput}
								>
									{#if approvingDevice}
										<span class="inline-block w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin mr-1"></span>
									{/if}
									Approve
								</button>
							</div>
						</div>
					</SettingsSection>

					<div class="border-t border-base-300 pt-6 mt-6">
						<SettingsSection title="Connected Devices" description="Devices that have access to your account">
							{#if devicesLoading}
								<div class="py-8 text-center">
									<div class="inline-block w-6 h-6 border-2 border-brand-500 border-t-transparent rounded-full animate-spin"></div>
								</div>
							{:else if devices.length === 0}
								<p class="py-4 text-sm text-base-content/60">No devices connected</p>
							{:else}
								<div class="space-y-3 py-4">
									{#each devices as device}
										<div class="flex items-center justify-between p-3 bg-base-100 rounded-lg border border-base-300">
											<div>
												<p class="text-sm font-medium text-base-content">{device.device_name}</p>
												<p class="text-xs text-base-content/50">Last active: {device.last_used_at ? formatDateTime(device.last_used_at) : 'Never'}</p>
											</div>
											<button
												type="button"
												class="px-3 py-1.5 text-sm font-medium text-error hover:bg-error/10 rounded-lg transition-colors"
												on:click={() => handleRevokeDevice(device.id)}
												disabled={revokingDeviceId === device.id}
											>
												{#if revokingDeviceId === device.id}
													<span class="inline-block w-4 h-4 border-2 border-error/30 border-t-error rounded-full animate-spin"></span>
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

					<div class="border-t border-base-300 pt-6 mt-6">
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
			<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
				<div class="p-6">
					<SettingsSection title="Theme" description="Choose how RustShare looks">
						<div class="grid grid-cols-1 sm:grid-cols-3 gap-4 py-4">
							<button
								type="button"
								class="p-4 rounded-xl border-2 text-left transition-all
									{$themeStore === 'light' ? 'border-brand-500 bg-brand-500/10' : 'border-base-300 hover:border-base-400'}"
								on:click={() => handleThemeChange('light')}
							>
								<div class="w-10 h-10 rounded-lg bg-base-100 border border-base-300 flex items-center justify-center mb-3">
									<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-5 h-5 text-yellow-500">
										<circle cx="12" cy="12" r="5"/>
										<path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/>
									</svg>
								</div>
								<p class="font-medium text-base-content">Light</p>
								<p class="text-xs text-base-content/60 mt-1">Always use light mode</p>
							</button>

							<button
								type="button"
								class="p-4 rounded-xl border-2 text-left transition-all
									{$themeStore === 'dark' ? 'border-brand-500 bg-brand-500/10' : 'border-base-300 hover:border-base-400'}"
								on:click={() => handleThemeChange('dark')}
							>
								<div class="w-10 h-10 rounded-lg bg-base-300 border border-base-400 flex items-center justify-center mb-3">
									<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-5 h-5 text-blue-400">
										<path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
									</svg>
								</div>
								<p class="font-medium text-base-content">Dark</p>
								<p class="text-xs text-base-content/60 mt-1">Always use dark mode</p>
							</button>

							<button
								type="button"
								class="p-4 rounded-xl border-2 text-left transition-all
									{$themeStore === 'system' ? 'border-brand-500 bg-brand-500/10' : 'border-base-300 hover:border-base-400'}"
								on:click={() => handleThemeChange('system')}
							>
								<div class="w-10 h-10 rounded-lg bg-gradient-to-br from-base-100 to-base-300 border border-base-300 flex items-center justify-center mb-3">
									<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-5 h-5 text-base-content">
										<rect x="2" y="3" width="20" height="14" rx="2" ry="2"/>
										<line x1="8" y1="21" x2="16" y2="21"/>
										<line x1="12" y1="17" x2="12" y2="21"/>
									</svg>
								</div>
								<p class="font-medium text-base-content">System</p>
								<p class="text-xs text-base-content/60 mt-1">Follow system preference</p>
							</button>
						</div>
					</SettingsSection>
				</div>
			</div>

		{:else if activeTab === 'sharing'}
			<!-- Sharing Tab -->
			<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
				<div class="p-6">
					<SettingsSection title="Default Link Settings" description="Preferences for new share links">
						<div class="py-4">
							<label class="block text-sm font-medium text-base-content mb-2">Default link expiration</label>
							<select 
								class="w-full sm:w-auto px-4 py-2 bg-base-100 border border-base-300 rounded-lg text-sm text-base-content focus:outline-none focus:border-brand-500/50"
								bind:value={defaultLinkExpiration}
							>
								<option value="0">Never</option>
								<option value="7">7 days</option>
								<option value="30">30 days</option>
								<option value="90">90 days</option>
							</select>
						</div>
						<div class="py-4 border-t border-base-300">
							<label class="flex items-center justify-between gap-4 cursor-pointer">
								<div class="flex-1">
									<p class="text-sm font-medium text-base-content">Require password for links</p>
									<p class="text-sm text-base-content/60">Add password protection to new share links by default</p>
								</div>
								<input type="checkbox" class="toggle toggle-primary" bind:checked={requirePasswordForLinks} />
							</label>
						</div>
						<div class="py-4 border-t border-base-300">
							<label class="flex items-center justify-between gap-4 cursor-pointer">
								<div class="flex-1">
									<p class="text-sm font-medium text-base-content">Allow public uploads</p>
									<p class="text-sm text-base-content/60">Allow upload-only links by default when creating folder shares</p>
								</div>
								<input type="checkbox" class="toggle toggle-primary" bind:checked={allowPublicUploads} />
							</label>
						</div>
					</SettingsSection>

					<div class="border-t border-base-300 pt-6 mt-6">
						<SettingsSection title="Email Sharing" description="Control how others can share with you">
							{#if profileLoading}
								<div class="py-4 text-center">
									<div class="inline-block w-5 h-5 border-2 border-brand-500 border-t-transparent rounded-full animate-spin"></div>
								</div>
							{:else}
								<div class="py-4">
									<label class="flex items-center justify-between gap-4 cursor-pointer">
										<div class="flex-1">
											<p class="text-sm font-medium text-base-content">Allow email sharing</p>
											<p class="text-sm text-base-content/60">Allow other users to see your email when sharing files</p>
										</div>
										<input 
											type="checkbox" 
											class="toggle toggle-primary" 
											bind:checked={emailSharingEnabled}
											on:change={handleEmailSharingToggle}
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
			<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
				<div class="p-6">
					<SettingsSection title="Recent Changes" description="Last 3 modified items in your workspace">
						{#if activityLoading}
							<div class="py-8 text-center">
								<div class="inline-block w-6 h-6 border-2 border-brand-500 border-t-transparent rounded-full animate-spin"></div>
							</div>
						{:else if recentChanges.length === 0}
							<div class="py-8 text-center">
								<Clock size={32} class="mx-auto mb-3 text-base-content/20" />
								<p class="text-sm text-base-content/60">No recent changes found</p>
							</div>
						{:else}
							<div class="py-2">
								{#each recentChanges as file, index}
									<button
										on:click={() => navigateToFile(file)}
										class="w-full flex items-center gap-3 p-3 rounded-xl hover:bg-base-100 transition-colors text-left"
									>
										<div class="flex h-9 w-9 items-center justify-center rounded-lg bg-base-100 border border-base-300 flex-shrink-0">
											<svelte:component this={getFileIcon(file.mime_type)} size={16} class="text-brand-500" />
										</div>
										<div class="flex-1 min-w-0">
											<p class="text-sm font-medium text-base-content truncate">{file.name}</p>
											<p class="text-xs text-base-content/50 flex items-center gap-1">
												<Folder size={10} />
												<span class="truncate">{file.path || 'Root'}</span>
											</p>
										</div>
										<div class="text-right flex-shrink-0">
											<p class="text-xs text-base-content/60">{formatDate(file.modified_at)}</p>
											<p class="text-[10px] text-base-content/40 uppercase tracking-wider">{formatFileSize(file.size)}</p>
										</div>
									</button>
									{#if index < recentChanges.length - 1}
										<div class="h-px bg-base-300/50 mx-3"></div>
									{/if}
								{/each}
							</div>
						{/if}
					</SettingsSection>

					<div class="border-t border-base-300 pt-6 mt-6">
						<p class="text-xs text-base-content/50 text-center">
							This shows a lightweight audit snapshot. For detailed file history, visit your <a href="/files" class="text-brand-500 hover:underline">Library</a>.
						</p>
					</div>
				</div>
			</div>
		{/if}
	</div>
</div>

{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => showToast = false} />
{/if}
