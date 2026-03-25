<script lang="ts">
	import { onMount } from 'svelte';
	import { currentUser, authStore } from '$lib/stores/auth';
	import { getAuthConfig, type AuthConfig } from '$lib/api/auth';
	import {
		listUserSecurityEvents,
		listUserSessions,
		revokeUserSession,
		updateUserPassword,
		listUserDevices,
		revokeUserDevice
	} from '$lib/api/users';
	import type { UserSecurityEvent, UserSession, UserDevice } from '$lib/api/users';
	import { approveDevicePairing } from '$lib/api/auth';
	import Toast from '$lib/components/common/Toast.svelte';

	let showToast = false;
	let toastMessage = '';
	let toastType: 'success' | 'error' | 'info' = 'info';
	let passwordForm = {
		current_password: '',
		new_password: '',
		confirm_password: ''
	};
	let passwordFormError = '';
	let passwordUpdating = false;
	let authConfigLoading = true;
	let authConfigError = '';
	let sessionsLoading = true;
	let sessionsError = '';
	let sessions: UserSession[] = [];
	let revokingSessionId: string | null = null;
	let securityEventsLoading = true;
	let securityEventsError = '';
	let securityEvents: UserSecurityEvent[] = [];
	let devices: UserDevice[] = [];
	let devicesLoading = true;
	let devicesError = '';
	let revokingDeviceId: string | null = null;
	let userCodeInput = '';
	let approvingDevice = false;
	let authConfig: AuthConfig = {
		password_login_enabled: true,
		oidc_enabled: false,
		oidc_login_label: null,
		oidc_mobile_enabled: false
	};

	// Theme management
	let currentTheme = 'light';

	// Get current theme from HTML element
	function getCurrentTheme() {
		if (typeof window !== 'undefined') {
			const html = document.documentElement;
			currentTheme = html.getAttribute('data-theme') || 'light';
		}
	}

	// Toggle theme
	function toggleTheme() {
		const newTheme = currentTheme === 'light' ? 'dark' : 'light';
		currentTheme = newTheme;

		if (typeof window !== 'undefined') {
			document.documentElement.setAttribute('data-theme', newTheme);
			localStorage.setItem('theme', newTheme);
		}

		showNotification(`Theme changed to ${newTheme} mode`, 'success');
	}

	// Initialize theme on mount
	if (typeof window !== 'undefined') {
		getCurrentTheme();
	}

	onMount(async () => {
		try {
			authConfig = await getAuthConfig();
		} catch (error) {
			authConfigError =
				error instanceof Error ? error.message : 'Failed to load authentication settings';
		} finally {
			authConfigLoading = false;
		}

		await refreshSessions();
		await refreshSecurityEvents();
		await refreshDevices();
	});

	async function refreshDevices() {
		devicesLoading = true;
		devicesError = '';
		try {
			devices = await listUserDevices();
		} catch (error) {
			devicesError = error instanceof Error ? error.message : 'Failed to load devices';
		} finally {
			devicesLoading = false;
		}
	}

	function formatPairingCode(input: string): string {
		// Remove any non-alphanumeric characters
		const cleaned = input.replace(/[^a-zA-Z0-9]/g, '').toUpperCase();
		
		// Add hyphen after first 4 characters if we have more than 4
		if (cleaned.length > 4) {
			return `${cleaned.slice(0, 4)}-${cleaned.slice(4, 8)}`;
		}
		
		return cleaned;
	}

	function handleCodeInput(event: Event) {
		const input = event.target as HTMLInputElement;
		const formatted = formatPairingCode(input.value);
		userCodeInput = formatted;
	}

	async function handleApproveDevice() {
		// Clean up input (remove dashes, spaces, make uppercase)
		const code = userCodeInput.replace(/[-\s]/g, '').toUpperCase();
		
		if (!code || code.length < 8) {
			showNotification('Please enter a valid 8-character pairing code', 'error');
			return;
		}

		approvingDevice = true;
		try {
			const response = await approveDevicePairing(code);
			userCodeInput = '';
			showNotification(`Device "${response.device_name}" approved successfully`, 'success');
			await refreshDevices();
			await refreshSecurityEvents();
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to approve device pairing',
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
			await refreshSecurityEvents();
			showNotification('Device revoked successfully', 'success');
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to revoke device',
				'error'
			);
		} finally {
			revokingDeviceId = null;
		}
	}

	async function refreshSessions() {
		sessionsLoading = true;
		sessionsError = '';
		try {
			sessions = await listUserSessions();
		} catch (error) {
			sessionsError = error instanceof Error ? error.message : 'Failed to load sessions';
		} finally {
			sessionsLoading = false;
		}
	}

	async function refreshSecurityEvents() {
		securityEventsLoading = true;
		securityEventsError = '';
		try {
			securityEvents = await listUserSecurityEvents();
		} catch (error) {
			securityEventsError =
				error instanceof Error ? error.message : 'Failed to load security activity';
		} finally {
			securityEventsLoading = false;
		}
	}

	function showNotification(message: string, type: 'success' | 'error' | 'info') {
		toastMessage = message;
		toastType = type;
		showToast = true;
	}

	async function handleLogout() {
		await authStore.logout();
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
			const response = await updateUserPassword(passwordForm);
			passwordForm = {
				current_password: '',
				new_password: '',
				confirm_password: ''
			};
			await refreshSecurityEvents();
			showNotification(response.message, 'success');
		} catch (error) {
			passwordFormError = error instanceof Error ? error.message : 'Failed to update password';
		} finally {
			passwordUpdating = false;
		}
	}

	async function handleRevokeSession(sessionId: string) {
		revokingSessionId = sessionId;
		try {
			await revokeUserSession(sessionId);
			await Promise.all([refreshSessions(), refreshSecurityEvents()]);
			showNotification('Session revoked successfully', 'success');
		} catch (error) {
			showNotification(
				error instanceof Error ? error.message : 'Failed to revoke session',
				'error'
			);
		} finally {
			revokingSessionId = null;
		}
	}

	function formatBytes(bytes: number | undefined): string {
		if (bytes === undefined) return 'N/A';
		if (bytes === 0) return '0 Bytes';

		const k = 1024;
		const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));

		return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i];
	}

	function formatDate(dateString: string | undefined): string {
		if (!dateString) return 'N/A';

		const date = new Date(dateString);
		return date.toLocaleDateString('en-US', {
			year: 'numeric',
			month: 'long',
			day: 'numeric'
		});
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

	function formatSecurityEventType(eventType: string): string {
		return eventType
			.split('_')
			.map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
			.join(' ');
	}

	$: storagePercentage =
		$currentUser?.storage_quota && $currentUser?.storage_used
			? Math.round(($currentUser.storage_used / $currentUser.storage_quota) * 100)
			: 0;
</script>

<svelte:head>
	<title>Settings - RustShare</title>
</svelte:head>

<div class="max-w-4xl space-y-6 mx-auto">
	<h1 class="text-2xl lg:text-3xl font-bold">Settings</h1>

	<!-- Profile Information -->
	<div class="card bg-base-100 shadow-xl">
		<div class="card-body">
			<h2 class="card-title text-xl mb-4">Profile Information</h2>

			<div class="space-y-4">
				<!-- Avatar -->
				<div class="gap-4 flex items-center">
					<div class="avatar placeholder">
						<div class="bg-primary text-primary-content w-20 h-20 rounded-full">
							<span class="text-3xl">{$currentUser?.display_name[0].toUpperCase()}</span>
						</div>
					</div>
					<div>
						<h3 class="text-lg font-semibold">{$currentUser?.display_name}</h3>
						<p class="text-sm text-base-content/70">{$currentUser?.email}</p>
					</div>
				</div>

				<div class="divider"></div>

				<!-- User Details -->
				<div class="md:grid-cols-2 gap-4 grid grid-cols-1">
					<div>
						<div class="label">
							<span class="label-text font-semibold">User ID</span>
						</div>
						<div class="text-sm text-base-content/70 font-mono">{$currentUser?.id}</div>
					</div>

					<div>
						<div class="label">
							<span class="label-text font-semibold">Account Type</span>
						</div>
						<div class="text-sm">
							{#if $currentUser?.is_admin}
								<span class="badge badge-primary">Administrator</span>
							{:else}
								<span class="badge">User</span>
							{/if}
						</div>
					</div>

					<div>
						<div class="label">
							<span class="label-text font-semibold">Member Since</span>
						</div>
						<div class="text-sm text-base-content/70">{formatDate($currentUser?.created_at)}</div>
					</div>

					<div>
						<div class="label">
							<span class="label-text font-semibold">Last Updated</span>
						</div>
						<div class="text-sm text-base-content/70">{formatDate($currentUser?.updated_at)}</div>
					</div>
				</div>
			</div>
		</div>
	</div>

	<!-- Storage Information -->
	<div class="card bg-base-100 shadow-xl">
		<div class="card-body">
			<h2 class="card-title text-xl mb-4">Storage</h2>

			{#if $currentUser?.storage_quota !== undefined && $currentUser?.storage_used !== undefined}
				<div class="space-y-4">
					<!-- Storage Usage Bar -->
					<div>
						<div class="text-sm mb-2 flex justify-between">
							<span>{formatBytes($currentUser.storage_used)} used</span>
							<span>{formatBytes($currentUser.storage_quota)} total</span>
						</div>
						<progress
							class="progress progress-primary w-full"
							value={$currentUser.storage_used}
							max={$currentUser.storage_quota}
						></progress>
						<div class="text-sm text-base-content/70 mt-2 text-center">
							{storagePercentage}% used
						</div>
					</div>

					<!-- Storage Details -->
					<div class="gap-4 grid grid-cols-2 text-center">
						<div class="stat bg-base-200 rounded-box p-4">
							<div class="stat-title">Used</div>
							<div class="stat-value text-lg">{formatBytes($currentUser.storage_used)}</div>
						</div>
						<div class="stat bg-base-200 rounded-box p-4">
							<div class="stat-title">Available</div>
							<div class="stat-value text-lg">
								{formatBytes(($currentUser.storage_quota || 0) - ($currentUser.storage_used || 0))}
							</div>
						</div>
					</div>
				</div>
			{:else}
				<div class="alert alert-info">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						class="w-6 h-6 shrink-0 stroke-current"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="2"
							d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
						></path>
					</svg>
					<span
						>Storage information is not available. Contact your administrator to enable storage
						quotas.</span
					>
				</div>
			{/if}
		</div>
	</div>

	<!-- Appearance -->
	<div class="card bg-base-100 shadow-xl">
		<div class="card-body">
			<h2 class="card-title text-xl mb-4">Appearance</h2>

			<div class="flex items-center justify-between">
				<div>
					<h3 class="font-semibold">Theme</h3>
					<p class="text-sm text-base-content/70">Switch between light and dark mode</p>
				</div>
				<div class="form-control">
					<label class="label gap-4 cursor-pointer">
						<span class="label-text">
							{#if currentTheme === 'light'}
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="w-6 h-6"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M12 3v2.25m6.364.386l-1.591 1.591M21 12h-2.25m-.386 6.364l-1.591-1.591M12 18.75V21m-4.773-4.227l-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0z"
									/>
								</svg>
							{:else}
								<svg
									xmlns="http://www.w3.org/2000/svg"
									fill="none"
									viewBox="0 0 24 24"
									stroke-width="1.5"
									stroke="currentColor"
									class="w-6 h-6"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										d="M21.752 15.002A9.718 9.718 0 0118 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 003 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 009.002-5.998z"
									/>
								</svg>
							{/if}
						</span>
						<input
							type="checkbox"
							class="toggle toggle-primary"
							checked={currentTheme === 'dark'}
							on:change={toggleTheme}
						/>
					</label>
				</div>
			</div>
		</div>
	</div>

	<!-- Security -->
	<div class="card bg-base-100 shadow-xl">
		<div class="card-body">
			<h2 class="card-title text-xl mb-4">Security</h2>

			<div class="space-y-4">
				<div class="bg-base-200 rounded-box p-4">
					<div class="mb-1 flex items-center justify-between">
						<h3 class="font-semibold">Current Web Session</h3>
						<span class="badge badge-success">Active</span>
					</div>
					<p class="text-sm text-base-content/70">
						This browser session is managed by RustShare with a secure HTTP-only cookie. The session
						is not stored in browser local storage.
					</p>
				</div>

				<div class="bg-base-200 rounded-box p-4">
					<div class="mb-3 flex items-center justify-between">
						<div>
							<h3 class="font-semibold">Enabled Sign-In Methods</h3>
							<p class="text-sm text-base-content/70">
								These options are currently available on this deployment.
							</p>
						</div>
						{#if authConfigLoading}
							<span class="loading loading-spinner loading-sm"></span>
						{/if}
					</div>

					{#if authConfigError}
						<div class="alert alert-warning py-2">
							<span>{authConfigError}</span>
						</div>
					{:else}
						<div class="gap-3 md:grid-cols-3 grid grid-cols-1">
							<div class="rounded-box border-base-300 p-3 border">
								<div class="mb-1 flex items-center justify-between">
									<span class="font-medium">Email + Password</span>
									<span
										class="badge {authConfig.password_login_enabled
											? 'badge-success'
											: 'badge-ghost'}"
									>
										{authConfig.password_login_enabled ? 'Enabled' : 'Disabled'}
									</span>
								</div>
								<p class="text-sm text-base-content/70">
									Direct login with your RustShare account password.
								</p>
							</div>

							<div class="rounded-box border-base-300 p-3 border">
								<div class="mb-1 flex items-center justify-between">
									<span class="font-medium">Single Sign-On</span>
									<span class="badge {authConfig.oidc_enabled ? 'badge-success' : 'badge-ghost'}">
										{authConfig.oidc_enabled ? 'Enabled' : 'Disabled'}
									</span>
								</div>
								<p class="text-sm text-base-content/70">
									{#if authConfig.oidc_enabled}
										{authConfig.oidc_login_label || 'Single Sign-On'} is available for browser login.
									{:else}
										No browser SSO provider is currently configured.
									{/if}
								</p>
							</div>

							<div class="rounded-box border-base-300 p-3 border">
								<div class="mb-1 flex items-center justify-between">
									<span class="font-medium">Mobile OIDC</span>
									<span
										class="badge {authConfig.oidc_mobile_enabled ? 'badge-success' : 'badge-ghost'}"
									>
										{authConfig.oidc_mobile_enabled ? 'Enabled' : 'Disabled'}
									</span>
								</div>
								<p class="text-sm text-base-content/70">
									PKCE-based mobile sign-in for lightweight RustShare clients.
								</p>
							</div>
						</div>
					{/if}
				</div>

				<div class="bg-base-200 rounded-box p-4">
					<div class="mb-3 flex items-center justify-between">
						<div>
							<h3 class="font-semibold">Active Browser Sessions</h3>
							<p class="text-sm text-base-content/70">
								Review recent browser sessions and revoke older ones.
							</p>
						</div>
						{#if sessionsLoading}
							<span class="loading loading-spinner loading-sm"></span>
						{:else}
							<button type="button" class="btn btn-ghost btn-sm" on:click={refreshSessions}>
								Refresh
							</button>
						{/if}
					</div>

					{#if sessionsError}
						<div class="alert alert-warning py-2">
							<span>{sessionsError}</span>
						</div>
					{:else if sessions.length === 0 && !sessionsLoading}
						<p class="text-sm text-base-content/70">No active browser sessions found.</p>
					{:else}
						<div class="space-y-3">
							{#each sessions as session}
								<div class="rounded-box border-base-300 p-3 border">
									<div class="mb-2 gap-4 flex items-start justify-between">
										<div>
											<div class="gap-2 flex items-center">
												<span class="font-medium">
													{session.user_agent || 'Browser session'}
												</span>
												{#if session.is_current}
													<span class="badge badge-primary">Current</span>
												{/if}
											</div>
											<div class="text-sm text-base-content/70">
												{session.ip_address || 'Unknown IP'}
											</div>
										</div>

										{#if !session.is_current}
											<button
												type="button"
												class="btn btn-outline btn-error btn-xs"
												on:click={() => handleRevokeSession(session.id)}
												disabled={revokingSessionId === session.id}
											>
												{#if revokingSessionId === session.id}
													<span class="loading loading-spinner loading-xs"></span>
												{/if}
												Revoke
											</button>
										{/if}
									</div>

									<div class="gap-2 text-sm text-base-content/70 md:grid-cols-3 grid grid-cols-1">
										<div>
											<span class="font-medium text-base-content">Started:</span>
											{formatDateTime(session.created_at)}
										</div>
										<div>
											<span class="font-medium text-base-content">Last seen:</span>
											{formatDateTime(session.last_seen_at)}
										</div>
										<div>
											<span class="font-medium text-base-content">Expires:</span>
											{formatDateTime(session.expires_at)}
										</div>
									</div>
								</div>
							{/each}
						</div>
					{/if}
				</div>

				<div class="bg-base-200 rounded-box p-4">
					<div class="mb-3 flex items-center justify-between">
						<div>
							<h3 class="font-semibold text-lg">Devices</h3>
							<p class="text-sm text-base-content/70">
								Manage devices paired with your account.
							</p>
						</div>
						<a href="/device" class="btn btn-primary btn-sm">
							<svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
								<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 4.5v15m7.5-7.5h-15" />
							</svg>
							Pair This Device
						</a>
					</div>

					<div class="space-y-6">
						<!-- Pair New Device -->
						<div class="p-4 bg-base-100 rounded-box border border-base-300">
							<h4 class="font-medium mb-2">Approve Pairing</h4>
							<p class="text-sm text-base-content/70 mb-4">
								Enter the 8-character pairing key from another device to approve it.
							</p>
							<div class="flex gap-2">
								<input
									type="text"
									placeholder="XXXX-XXXX"
									class="input input-bordered w-full max-w-xs font-mono"
									value={userCodeInput}
									on:input={handleCodeInput}
									maxlength="9"
								/>
								<button
									class="btn btn-primary"
									on:click={handleApproveDevice}
									disabled={approvingDevice || !userCodeInput}
								>
									{#if approvingDevice}
										<span class="loading loading-spinner loading-xs"></span>
									{/if}
									Approve
								</button>
							</div>
						</div>

						<!-- Device List -->
						<div class="space-y-3">
							<div class="flex items-center justify-between">
								<h4 class="font-medium">Active Devices</h4>
								{#if devicesLoading}
									<span class="loading loading-spinner loading-sm"></span>
								{:else}
									<button type="button" class="btn btn-ghost btn-xs" on:click={refreshDevices}>
										Refresh
									</button>
								{/if}
							</div>

							{#if devicesError}
								<div class="alert alert-warning py-2">
									<span>{devicesError}</span>
								</div>
							{:else if devices.length === 0 && !devicesLoading}
								<p class="text-sm text-base-content/70 italic">No other devices paired.</p>
							{:else}
								{#each devices as device}
									<div class="rounded-box border-base-300 p-3 border bg-base-100">
										<div class="mb-2 gap-4 flex items-start justify-between">
											<div>
												<div class="gap-2 flex items-center">
													<span class="font-medium">{device.device_name}</span>
												</div>
												<div class="text-xs text-base-content/70">
													ID: {device.id}
												</div>
											</div>

											<button
												type="button"
												class="btn btn-outline btn-error btn-xs"
												on:click={() => handleRevokeDevice(device.id)}
												disabled={revokingDeviceId === device.id}
											>
												{#if revokingDeviceId === device.id}
													<span class="loading loading-spinner loading-xs"></span>
												{/if}
												Revoke
											</button>
										</div>

										<div class="gap-2 text-sm text-base-content/70 md:grid-cols-2 grid grid-cols-1">
											<div>
												<span class="font-medium text-base-content">Paired:</span>
												{formatDateTime(device.created_at)}
											</div>
											<div>
												<span class="font-medium text-base-content">Last active:</span>
												{device.last_used_at ? formatDateTime(device.last_used_at) : 'Never'}
											</div>
										</div>
									</div>
								{/each}
							{/if}
						</div>
					</div>
				</div>

				<div class="bg-base-200 rounded-box p-4">
					<div class="mb-3 flex items-center justify-between">
						<div>
							<h3 class="font-semibold">Recent Security Activity</h3>
							<p class="text-sm text-base-content/70">
								Recent sign-in and account security changes for this account.
							</p>
						</div>
						{#if securityEventsLoading}
							<span class="loading loading-spinner loading-sm"></span>
						{:else}
							<button type="button" class="btn btn-ghost btn-sm" on:click={refreshSecurityEvents}>
								Refresh
							</button>
						{/if}
					</div>

					{#if securityEventsError}
						<div class="alert alert-warning py-2">
							<span>{securityEventsError}</span>
						</div>
					{:else if securityEvents.length === 0 && !securityEventsLoading}
						<p class="text-sm text-base-content/70">No recent security activity found.</p>
					{:else}
						<div class="space-y-3">
							{#each securityEvents as event}
								<div class="rounded-box border-base-300 p-3 border">
									<div class="mb-2 gap-3 flex items-start justify-between">
										<div>
											<div class="gap-2 flex items-center">
												<span class="font-medium">{event.description}</span>
												<span class="badge badge-ghost">
													{formatSecurityEventType(event.event_type)}
												</span>
											</div>
											<div class="text-sm text-base-content/70">
												{formatDateTime(event.occurred_at)}
											</div>
										</div>
									</div>

									<div class="gap-2 text-sm text-base-content/70 md:grid-cols-3 grid grid-cols-1">
										<div>
											<span class="font-medium text-base-content">IP:</span>
											{event.ip_address || 'Unknown'}
										</div>
										<div>
											<span class="font-medium text-base-content">Session:</span>
											{event.session_id || 'N/A'}
										</div>
										<div>
											<span class="font-medium text-base-content">Client:</span>
											{event.user_agent || 'Unknown client'}
										</div>
									</div>
								</div>
							{/each}
						</div>
					{/if}
				</div>
			</div>
		</div>
	</div>

	<!-- Account Actions -->
	<div class="card bg-base-100 shadow-xl">
		<div class="card-body">
			<h2 class="card-title text-xl mb-4">Account</h2>

			<div class="space-y-4">
				<div class="p-4 bg-base-200 rounded-box space-y-4">
					<div>
						<h3 class="font-semibold">Change Password</h3>
						<p class="text-sm text-base-content/70">
							Update the password used for direct email and password login
						</p>
					</div>

					{#if passwordFormError}
						<div class="alert alert-error py-2">
							<span>{passwordFormError}</span>
						</div>
					{/if}

					<div class="md:grid-cols-3 gap-4 grid grid-cols-1">
						<label class="form-control">
							<span class="label-text font-medium">Current Password</span>
							<input
								type="password"
								class="input input-bordered"
								bind:value={passwordForm.current_password}
								autocomplete="current-password"
							/>
						</label>

						<label class="form-control">
							<span class="label-text font-medium">New Password</span>
							<input
								type="password"
								class="input input-bordered"
								bind:value={passwordForm.new_password}
								autocomplete="new-password"
							/>
						</label>

						<label class="form-control">
							<span class="label-text font-medium">Confirm Password</span>
							<input
								type="password"
								class="input input-bordered"
								bind:value={passwordForm.confirm_password}
								autocomplete="new-password"
							/>
						</label>
					</div>

					<div class="gap-4 flex items-center justify-between">
						<p class="text-sm text-base-content/70">
							Passwords must be at least 10 characters and different from your current password.
						</p>
						<button
							class="btn btn-primary btn-sm"
							on:click={handlePasswordChange}
							disabled={passwordUpdating}
						>
							{#if passwordUpdating}
								<span class="loading loading-spinner loading-xs"></span>
							{/if}
							Update Password
						</button>
					</div>
				</div>

				<!-- Logout -->
				<div class="p-4 bg-base-200 rounded-box flex items-center justify-between">
					<div>
						<h3 class="font-semibold">Sign Out</h3>
						<p class="text-sm text-base-content/70">Sign out of your account</p>
					</div>
					<button class="btn btn-error btn-sm" on:click={handleLogout}> Logout </button>
				</div>
			</div>
		</div>
	</div>
</div>

<!-- Toast Notifications -->
{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => (showToast = false)} />
{/if}
