<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import {
		getProfile,
		updateProfile,
		uploadAvatar,
		deleteAvatar,
		getAvatarUrl,
		type FullUserProfile
	} from '$lib/api/users';
	import { authStore } from '$lib/stores/auth';
	import Toast from '$lib/components/common/Toast.svelte';

	// State
	let profile: FullUserProfile | null = $state(null);
	let loading = $state(true);
	let saving = $state(false);
	let uploadingAvatar = $state(false);
	let deletingAvatar = $state(false);

	// Form fields
	let name = $state('');
	let surname = $state('');
	let displayName = $state('');
	let emailSharingEnabled = $state(true);

	// Avatar
	let selectedFile: File | null = $state(null);
	let previewUrl: string | null = $state(null);
	let avatarTimestamp = $state(Date.now());
	let avatarUrl = $derived(profile && profile.avatar_path ? `${getAvatarUrl(profile.id)}?t=${avatarTimestamp}` : null);

	// Messages
	let successMessage = $state('');
	let errorMessage = $state('');
	let showSuccess = $state(false);
	let showError = $state(false);

	onMount(async () => {
		await loadProfile();
	});

	async function loadProfile() {
		loading = true;
		try {
			profile = await getProfile();
			name = profile.name || '';
			surname = profile.surname || '';
			displayName = profile.display_name || '';
			emailSharingEnabled = profile.email_sharing_enabled;
		} catch (e: any) {
			showErrorMessage(e.message || 'Failed to load profile');
		} finally {
			loading = false;
		}
	}

	function showSuccessMessage(msg: string) {
		successMessage = msg;
		showSuccess = true;
		setTimeout(() => (showSuccess = false), 3000);
	}

	function showErrorMessage(msg: string) {
		errorMessage = msg;
		showError = true;
		setTimeout(() => (showError = false), 3000);
	}

	async function handleSave() {
		if (!profile) return;

		saving = true;
		try {
			const updated = await updateProfile({
				name: name || undefined,
				surname: surname || undefined,
				display_name: displayName || undefined,
				email_sharing_enabled: emailSharingEnabled
			});
			// Preserve avatar_path if it's missing in the response but present in local state
			profile = {
				...updated,
				avatar_path: updated.avatar_path ?? profile.avatar_path
			};
			showSuccessMessage('Profile updated successfully');
			await authStore.refreshSession();
		} catch (e: any) {
			showErrorMessage(e.message || 'Failed to update profile');
		} finally {
			saving = false;
		}
	}

	function handleFileSelect(event: Event) {
		const input = event.target as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;

		// Validate file type
		if (!file.type.startsWith('image/')) {
			showErrorMessage('Please select an image file');
			return;
		}

		// Validate file size (max 5MB)
		if (file.size > 5 * 1024 * 1024) {
			showErrorMessage('Image must be less than 5MB');
			return;
		}

		selectedFile = file;

		// Create preview
		const reader = new FileReader();
		reader.onload = (e) => {
			previewUrl = e.target?.result as string;
		};
		reader.readAsDataURL(file);
	}

	async function handleAvatarUpload() {
		if (!selectedFile) {
			showErrorMessage('Please select an image first');
			return;
		}

		uploadingAvatar = true;
		try {
			await uploadAvatar(selectedFile);
			showSuccessMessage('Avatar uploaded successfully');
			selectedFile = null;
			previewUrl = null;
			// Update timestamp to bust cache
			avatarTimestamp = Date.now();
			// Reload profile to get updated avatar_path
			await loadProfile();
			// Update global session to update header avatar
			await authStore.refreshSession();
		} catch (e: any) {
			showErrorMessage(e.message || 'Failed to upload avatar');
		} finally {
			uploadingAvatar = false;
		}
	}

	async function handleAvatarDelete() {
		if (!profile?.avatar_path) return;

		deletingAvatar = true;
		try {
			await deleteAvatar();
			showSuccessMessage('Avatar deleted successfully');
			// Update timestamp to bust cache
			avatarTimestamp = Date.now();
			await loadProfile();
			await authStore.refreshSession();
		} catch (e: any) {
			showErrorMessage(e.message || 'Failed to delete avatar');
		} finally {
			deletingAvatar = false;
		}
	}

	function getInitials(): string {
		if (!profile) return '?';
		if (name && surname) {
			return `${name[0]}${surname[0]}`.toUpperCase();
		}
		return (profile.display_name?.[0] || profile.email[0] || '?').toUpperCase();
	}

	function formatDate(dateStr: string): string {
		return new Date(dateStr).toLocaleDateString();
	}
</script>

<svelte:head>
	<title>Profile - RustShare</title>
</svelte:head>

<div class="container mx-auto max-w-2xl p-4">
	<div class="card bg-base-100 shadow-xl">
		<div class="card-body">
			<h1 class="card-title text-2xl mb-6">Profile</h1>

			{#if loading}
				<div class="flex justify-center py-8">
					<span class="loading loading-spinner loading-lg text-primary"></span>
				</div>
			{:else if profile}
				<!-- Avatar Section -->
				<div class="flex flex-col items-center gap-4 mb-8">
					<div class="avatar">
						<div class="w-24 rounded-full ring ring-primary ring-offset-base-100 ring-offset-2">
							{#if previewUrl}
								<img src={previewUrl} alt="Avatar preview" />
							{:else if avatarUrl}
								<img src={avatarUrl} alt="Avatar" />
							{:else}
								<div class="bg-neutral text-neutral-content w-full h-full flex items-center justify-center text-2xl font-bold">
									{getInitials()}
								</div>
							{/if}
						</div>
					</div>

					<div class="flex flex-col items-center gap-2 w-full max-w-xs">
						<input
							id="avatar-input"
							type="file"
							accept="image/*"
							class="file-input file-input-bordered w-full"
							onchange={handleFileSelect}
						/>

						{#if selectedFile}
							<button
								class="btn btn-primary btn-sm w-full"
								onclick={handleAvatarUpload}
								disabled={uploadingAvatar}
							>
								{#if uploadingAvatar}
									<span class="loading loading-spinner loading-sm"></span>
								{/if}
								Upload Avatar
							</button>
						{/if}

						{#if profile.avatar_path}
							<button
								class="btn btn-error btn-outline btn-sm w-full"
								onclick={handleAvatarDelete}
								disabled={deletingAvatar}
							>
								{#if deletingAvatar}
									<span class="loading loading-spinner loading-sm"></span>
								{/if}
								Delete Avatar
							</button>
						{/if}
					</div>
				</div>

				<div class="divider"></div>

				<!-- Profile Form -->
				<div class="space-y-4">
					<!-- Username (read-only) -->
					<div class="form-control">
						<label class="label" for="username">
							<span class="label-text">Username</span>
						</label>
						<input
							id="username"
							type="text"
							class="input input-bordered"
							value={profile.username}
							disabled
						/>
						<label class="label">
							<span class="label-text-alt text-base-content/60">
								Username can only be changed once every 30 days
							</span>
						</label>
					</div>

					<!-- Email (read-only) -->
					<div class="form-control">
						<label class="label" for="email">
							<span class="label-text">Email</span>
						</label>
						<input
							id="email"
							type="email"
							class="input input-bordered"
							value={profile.email}
							disabled
						/>
					</div>

					<!-- Name -->
					<div class="form-control">
						<label class="label" for="first-name">
							<span class="label-text">First Name</span>
						</label>
						<input
							id="first-name"
							type="text"
							class="input input-bordered"
							placeholder="Enter your first name"
							maxlength="255"
							bind:value={name}
						/>
					</div>

					<!-- Surname -->
					<div class="form-control">
						<label class="label" for="last-name">
							<span class="label-text">Last Name</span>
						</label>
						<input
							id="last-name"
							type="text"
							class="input input-bordered"
							placeholder="Enter your last name"
							maxlength="255"
							bind:value={surname}
						/>
					</div>

					<!-- Display Name -->
					<div class="form-control">
						<label class="label" for="display-name">
							<span class="label-text">Display Name</span>
						</label>
						<input
							id="display-name"
							type="text"
							class="input input-bordered"
							placeholder="How you want to be called"
							maxlength="255"
							bind:value={displayName}
						/>
					</div>

					<!-- Email Sharing Toggle -->
					<div class="form-control">
						<label class="label cursor-pointer justify-start gap-4" for="email-sharing">
							<input
								id="email-sharing"
								type="checkbox"
								class="toggle toggle-primary"
								bind:checked={emailSharingEnabled}
							/>
							<div class="flex flex-col">
								<span class="label-text font-medium">Allow email sharing</span>
								<span class="label-text-alt text-base-content/60">
									Allow other users to see your email when sharing files
								</span>
							</div>
						</label>
					</div>

					<!-- Member Since -->
					<div class="form-control">
						<label class="label" for="member-since">
							<span class="label-text">Member Since</span>
						</label>
						<input
							id="member-since"
							type="text"
							class="input input-bordered"
							value={formatDate(profile.created_at)}
							disabled
						/>
					</div>

					<!-- Save Button -->
					<div class="pt-4">
						<button
							class="btn btn-primary w-full"
							onclick={handleSave}
							disabled={saving}
						>
							{#if saving}
								<span class="loading loading-spinner loading-sm mr-2"></span>
							{/if}
							Save Changes
						</button>
					</div>
				</div>
			{/if}
		</div>
	</div>
</div>

{#if showSuccess}
	<Toast message={successMessage} type="success" onClose={() => (showSuccess = false)} />
{/if}

{#if showError}
	<Toast message={errorMessage} type="error" onClose={() => (showError = false)} />
{/if}
