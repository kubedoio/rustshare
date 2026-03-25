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
	import { formatDate, formatFileSize } from '$lib/utils/format';

	let showToast = false;
	let toastMessage = '';
	let toastType: 'success' | 'error' | 'info' = 'info';
	let passwordForm = { current_password: '', new_password: '', confirm_password: '' };
	let passwordFormError = '';
	let passwordUpdating = false;
	let authConfigLoading = true;
	let authConfig: AuthConfig = {
		password_login_enabled: true,
		oidc_enabled: false,
		oidc_login_label: null,
		oidc_mobile_enabled: false
	};
	let sessions: UserSession[] = [];
	let sessionsLoading = true;
	let revokingSessionId: string | null = null;
	let securityEvents: UserSecurityEvent[] = [];
	let securityEventsLoading = true;
	let devices: UserDevice[] = [];
	let devicesLoading = true;
	let revokingDeviceId: string | null = null;
	let userCodeInput = '';
	let approvingDevice = false;

	$: storagePercentage = $currentUser?.storage_quota && $currentUser?.storage_used
		? Math.round(($currentUser.storage_used / $currentUser.storage_quota) * 100)
		: 0;

	onMount(async () => {
		try {
			authConfig = await getAuthConfig();
		} catch (error) {
			console.error('Failed to load auth config:', error);
		} finally {
			authConfigLoading = false;
		}
		await refreshSessions();
		await refreshSecurityEvents();
		await refreshDevices();
	});

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

	function formatPairingCode(input: string): string {
		const cleaned = input.replace(/[^a-zA-Z0-9]/g, '').toUpperCase();
		if (cleaned.length > 4) {
			return `${cleaned.slice(0, 4)}-${cleaned.slice(4, 8)}`;
		}
		return cleaned;
	}

	function handleCodeInput(event: Event) {
		const input = event.target as HTMLInputElement;
		userCodeInput = formatPairingCode(input.value);
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
			await refreshSecurityEvents();
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
			await refreshSecurityEvents();
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

	async function refreshSecurityEvents() {
		securityEventsLoading = true;
		try {
			securityEvents = await listUserSecurityEvents();
		} catch (error) {
			console.error('Failed to load security events:', error);
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
			const response = await updateUserPassword(passwordForm);
			passwordForm = { current_password: '', new_password: '', confirm_password: '' };
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
			showNotification('Session revoked', 'success');
		} catch (error) {
			showNotification(error instanceof Error ? error.message : 'Failed to revoke session', 'error');
		} finally {
			revokingSessionId = null;
		}
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
		return eventType.split('_').map(s => s.charAt(0).toUpperCase() + s.slice(1)).join(' ');
	}
</script>

<svelte:head>
	<title>Settings - RustShare</title>
</svelte:head>

<div class="max-w-5xl mx-auto space-y-6">
	<!-- Header -->
	<div>
		<h1 class="text-2xl font-semibold text-base-content">Settings</h1>
		<p class="text-base-content/60 mt-1">Manage your account and preferences</p>
	</div>

	<!-- Profile Card -->
	<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
		<div class="px-6 py-5 border-b border-base-300">
			<h2 class="font-semibold text-base-content">Profile</h2>
		</div>
		<div class="p-6">
			<div class="flex items-start gap-5">
				<div class="w-16 h-16 rounded-2xl bg-gradient-to-br from-brand-500 to-brand-600 flex items-center justify-center text-white text-2xl font-semibold flex-shrink-0">
					{$currentUser?.display_name?.charAt(0).toUpperCase() || '?'}
				</div>
				<div class="flex-1 min-w-0">
					<h3 class="text-lg font-semibold text-base-content">{$currentUser?.display_name}</h3>
					<p class="text-base-content/60">{$currentUser?.email}</p>
					<div class="flex flex-wrap gap-3 mt-3">
						{#if $currentUser?.is_admin}
							<span class="inline-flex items-center px-2.5 py-1 rounded-lg text-xs font-medium bg-brand-500/10 text-brand-400 border border-brand-500/20">
								Administrator
							</span>
						{/if}
						<span class="inline-flex items-center px-2.5 py-1 rounded-lg text-xs font-medium bg-base-300 text-base-content/70">
							ID: {$currentUser?.id?.slice(0, 8)}...
						</span>
					</div>
				</div>
				<a href="/profile" class="px-4 py-2 text-sm font-medium bg-base-300 hover:bg-base-300/80 text-base-content rounded-lg transition-colors">
					Edit Profile
				</a>
			</div>
		</div>
	</div>

	<!-- Storage Card -->
	{#if $currentUser?.storage_quota}
		<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
			<div class="px-6 py-5 border-b border-base-300">
				<h2 class="font-semibold text-base-content">Storage</h2>
			</div>
			<div class="p-6">
				<div class="flex items-center justify-between mb-3">
					<span class="text-sm text-base-content/60">{formatFileSize($currentUser.storage_used)} of {formatFileSize($currentUser.storage_quota)} used</span>
					<span class="text-sm font-medium text-base-content">{storagePercentage}%</span>
				</div>
				<div class="h-2 bg-base-300 rounded-full overflow-hidden">
					<div class="h-full bg-brand-500 rounded-full transition-all" style="width: {Math.min(storagePercentage, 100)}%"></div>
				</div>
			</div>
		</div>
	{/if}

	<!-- Security Card -->
	<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
		<div class="px-6 py-5 border-b border-base-300">
			<h2 class="font-semibold text-base-content">Security</h2>
		</div>
		<div class="p-6 space-y-6">
			<!-- Change Password -->
			<div>
				<h3 class="text-sm font-medium text-base-content mb-3">Change Password</h3>
				{#if passwordFormError}
					<div class="mb-4 p-3 bg-error/10 border border-error/20 rounded-lg text-sm text-error">
						{passwordFormError}
					</div>
				{/if}
				<div class="grid grid-cols-1 md:grid-cols-3 gap-4">
					<input
						type="password"
						placeholder="Current password"
						class="px-4 py-2 bg-base-100 border border-base-300 rounded-lg text-sm text-base-content placeholder:text-base-content/40 focus:outline-none focus:border-brand-500/50"
						bind:value={passwordForm.current_password}
					/>
					<input
						type="password"
						placeholder="New password"
						class="px-4 py-2 bg-base-100 border border-base-300 rounded-lg text-sm text-base-content placeholder:text-base-content/40 focus:outline-none focus:border-brand-500/50"
						bind:value={passwordForm.new_password}
					/>
					<input
						type="password"
						placeholder="Confirm password"
						class="px-4 py-2 bg-base-100 border border-base-300 rounded-lg text-sm text-base-content placeholder:text-base-content/40 focus:outline-none focus:border-brand-500/50"
						bind:value={passwordForm.confirm_password}
					/>
				</div>
				<div class="flex items-center justify-between mt-4">
					<p class="text-xs text-base-content/50">Password must be at least 10 characters</p>
					<button
						type="button"
						class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors disabled:opacity-50"
						on:click={handlePasswordChange}
						disabled={passwordUpdating}
					>
						{#if passwordUpdating}
							<span class="inline-block w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin mr-2"></span>
						{/if}
						Update Password
					</button>
				</div>
			</div>

			<!-- Device Pairing -->
			<div class="border-t border-base-300 pt-6">
				<div class="flex items-start justify-between mb-4">
					<div>
						<h3 class="text-sm font-medium text-base-content">Device Pairing</h3>
						<p class="text-xs text-base-content/50 mt-1">Pair mobile devices with your account</p>
					</div>
					<a href="/device" class="px-3 py-1.5 text-sm font-medium bg-base-300 hover:bg-base-300/80 text-base-content rounded-lg transition-colors">
						Pair This Device
					</a>
				</div>

				<div class="bg-base-100 rounded-lg border border-base-300 p-4 mb-4">
					<p class="text-sm text-base-content/70 mb-3">Enter an 8-character pairing code to approve a device</p>
					<div class="flex gap-3">
						<input
							type="text"
							placeholder="XXXX-XXXX"
							class="px-4 py-2 bg-base-200 border border-base-300 rounded-lg text-sm font-mono text-base-content placeholder:text-base-content/40 focus:outline-none focus:border-brand-500/50 w-32"
							value={userCodeInput}
							on:input={handleCodeInput}
							maxlength="9"
						/>
						<button
							type="button"
							class="px-4 py-2 text-sm font-medium bg-brand-500 hover:bg-brand-600 text-white rounded-lg transition-colors disabled:opacity-50"
							on:click={handleApproveDevice}
							disabled={approvingDevice || !userCodeInput}
						>
							{#if approvingDevice}
								<span class="inline-block w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin mr-2"></span>
							{/if}
							Approve
						</button>
					</div>
				</div>

				<!-- Device List -->
				{#if devices.length > 0}
					<div class="space-y-2">
						<p class="text-sm font-medium text-base-content mb-2">Active Devices</p>
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
			</div>
		</div>
	</div>

	<!-- Sessions Card -->
	<div class="bg-base-200 rounded-xl border border-base-300 overflow-hidden">
		<div class="px-6 py-5 border-b border-base-300 flex items-center justify-between">
			<h2 class="font-semibold text-base-content">Active Sessions</h2>
			<button
				type="button"
				class="text-sm text-brand-400 hover:text-brand-300 transition-colors"
				on:click={refreshSessions}
			>
				Refresh
			</button>
		</div>
		<div class="divide-y divide-base-300">
			{#if sessionsLoading}
				<div class="p-8 flex justify-center">
					<div class="animate-spin h-6 w-6 border-2 border-brand-500 border-t-transparent rounded-full"></div>
				</div>
			{:else if sessions.length === 0}
				<div class="p-6 text-center text-base-content/60">
					No active sessions
				</div>
			{:else}
				{#each sessions as session}
					<div class="px-6 py-4 flex items-start justify-between">
						<div>
							<div class="flex items-center gap-2">
								<p class="text-sm font-medium text-base-content">{session.user_agent || 'Browser'}</p>
								{#if session.is_current}
									<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-success/10 text-success">
										Current
									</span>
								{/if}
							</div>
							<p class="text-xs text-base-content/50 mt-1">
								{session.ip_address || 'Unknown IP'} • Started {formatDateTime(session.created_at)}
							</p>
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
			{/if}
		</div>
	</div>

	<!-- Sign Out -->
	<div class="flex justify-end pt-4">
		<button
			type="button"
			class="px-4 py-2 text-sm font-medium text-error hover:bg-error/10 rounded-lg transition-colors"
			on:click={handleLogout}
		>
			Sign out
		</button>
	</div>
</div>

{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => showToast = false} />
{/if}
