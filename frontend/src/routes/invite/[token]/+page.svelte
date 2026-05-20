<script lang="ts">
	import { page } from '$app/stores';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { browser } from '$app/environment';
	import { getInvite, acceptInvite, type InviteDetail } from '$lib/api/invites';
	import type { User } from '$lib/api/types';

	let token = $page.params.token ?? '';
	let workflow: InviteDetail | null = null;
	let parseError = false;
	let submitError = '';
	let isSubmitting = false;
	let submitted = false;
	let createdUser: User | null = null;

	// Form state
	let displayName = '';
	let email = '';
	let password = '';
	let confirmPassword = '';
	let termsAccepted = false;

	// UI state
	let currentStep: 'form' | 'terms' = 'form';

	onMount(() => {
		void (async () => {
			if (!browser) return;
			try {
				workflow = await getInvite(token);
				email = workflow.recipient_email;
			} catch {
				parseError = true;
			}
		})();
	});

	function validateForm(): string {
		if (!displayName.trim()) return 'Please enter your full name.';
		if (!email.trim()) return 'Please enter your email address.';
		if (!password) return 'Please choose a password.';
		if (password.length < 8) return 'Password must be at least 8 characters.';
		if (password !== confirmPassword) return 'Passwords do not match.';
		if (workflow?.terms_enabled && !termsAccepted) return 'Please accept the Terms & Conditions.';
		return '';
	}

	async function handleSubmit() {
		submitError = '';
		const err = validateForm();
		if (err) {
			submitError = err;
			return;
		}

		isSubmitting = true;
		try {
			const user = await acceptInvite(token, {
				display_name: displayName.trim(),
				email: email.trim(),
				password,
				terms_accepted: termsAccepted
			});
			createdUser = user;
			submitted = true;
			setTimeout(() => goto('/login'), 2500);
		} catch (err: any) {
			if (err?.status === 409) {
				submitError = 'This email already has an account. Please sign in instead.';
			} else {
				submitError = err?.message || 'Failed to create account. Please try again.';
			}
		} finally {
			isSubmitting = false;
		}
	}
</script>

<svelte:head>
	<title>You're Invited — RustShare</title>
	<meta
		name="description"
		content="Accept your invitation to join RustShare, a secure file sharing platform."
	/>
</svelte:head>

<div
	class="flex min-h-screen items-center justify-center bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 p-4"
>
	<!-- Background decoration -->
	<div class="pointer-events-none absolute inset-0 overflow-hidden">
		<div class="absolute -top-40 -right-40 h-96 w-96 rounded-full bg-brand-500/10 blur-3xl"></div>
		<div
			class="absolute -bottom-40 -left-40 h-96 w-96 rounded-full bg-purple-500/10 blur-3xl"
		></div>
	</div>

	<div class="relative w-full max-w-md">
		{#if parseError}
			<!-- Invalid / Expired Token -->
			<div
				class="rounded-3xl border border-white/10 bg-white/5 p-8 text-center shadow-2xl backdrop-blur-xl"
			>
				<div
					class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-red-500/20"
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						class="h-8 w-8 text-red-400"
						fill="none"
						viewBox="0 0 24 24"
						stroke="currentColor"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="2"
							d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z"
						/>
					</svg>
				</div>
				<h1 class="mb-2 text-xl font-bold text-white">Invite Link Expired or Invalid</h1>
				<p class="mb-6 text-sm text-white/50">
					This invite link has expired (links are valid for 7 days) or is not valid. Please ask your
					contact to send a new invitation.
				</p>
				<a
					href="/login"
					class="inline-block text-sm font-bold text-brand-400 transition-colors hover:text-brand-300"
					>Back to Login →</a
				>
			</div>
		{:else if submitted}
			<!-- Success State -->
			<div
				class="rounded-3xl border border-white/10 bg-white/5 p-8 text-center shadow-2xl backdrop-blur-xl"
			>
				<div
					class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-green-500/20"
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						class="h-8 w-8 text-green-400"
						fill="none"
						viewBox="0 0 24 24"
						stroke="currentColor"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="2"
							d="M5 13l4 4L19 7"
						/>
					</svg>
				</div>
				<h1 class="mb-2 text-xl font-bold text-white">Welcome to RustShare!</h1>
				<p class="mb-3 text-sm text-white/50">
					Your account is being created. Redirecting you to login...
				</p>
				<div class="flex items-center justify-center gap-2">
					<div
						class="h-1.5 w-1.5 animate-bounce rounded-full bg-green-400"
						style="animation-delay:0ms"
					></div>
					<div
						class="h-1.5 w-1.5 animate-bounce rounded-full bg-green-400"
						style="animation-delay:150ms"
					></div>
					<div
						class="h-1.5 w-1.5 animate-bounce rounded-full bg-green-400"
						style="animation-delay:300ms"
					></div>
				</div>
			</div>
		{:else if workflow}
			<!-- Invite Form -->
			<div
				class="overflow-hidden rounded-3xl border border-white/10 bg-white/5 shadow-2xl backdrop-blur-xl"
			>
				<!-- Header -->
				<div class="border-b border-white/10 px-7 pt-7 pb-5">
					<div class="mb-4 flex items-center gap-3">
						<div
							class="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-500 shadow-lg shadow-brand-500/30"
						>
							<svg
								xmlns="http://www.w3.org/2000/svg"
								class="h-5 w-5 text-white"
								fill="none"
								viewBox="0 0 24 24"
								stroke="currentColor"
								stroke-width="2"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									d="M13.19 8.688a4.5 4.5 0 011.242 7.244l-4.5 4.5a4.5 4.5 0 01-6.364-6.364l1.757-1.757m13.35-.622l1.757-1.757a4.5 4.5 0 00-6.364-6.364l-4.5 4.5a4.5 4.5 0 001.242 7.244"
								/>
							</svg>
						</div>
						<span class="text-sm font-semibold text-white/60">RustShare</span>
					</div>
					<h1 class="text-2xl leading-tight font-bold text-white">{workflow.subject}</h1>
					<p class="mt-1.5 text-sm text-white/50">{workflow.body}</p>
				</div>

				<!-- Form -->
				<div class="space-y-4 px-7 py-5">
					<!-- Step indicator if T&C required -->
					{#if workflow.terms_enabled}
						<div class="mb-1 flex items-center gap-2">
							<button
								type="button"
								class="flex items-center gap-1.5 text-xs font-semibold transition-colors {currentStep ===
								'form'
									? 'text-brand-400'
									: 'text-white/40'}"
								onclick={() => (currentStep = 'form')}
							>
								<span
									class="flex h-5 w-5 items-center justify-center rounded-full text-[10px] {currentStep ===
									'form'
										? 'bg-brand-500 text-white'
										: 'bg-white/10 text-white/50'}">1</span
								>
								Your Details
							</button>
							<div class="h-px flex-1 bg-white/10"></div>
							<button
								type="button"
								class="flex items-center gap-1.5 text-xs font-semibold transition-colors {currentStep ===
								'terms'
									? 'text-brand-400'
									: 'text-white/40'}"
							>
								<span
									class="flex h-5 w-5 items-center justify-center rounded-full text-[10px] {currentStep ===
									'terms'
										? 'bg-brand-500 text-white'
										: 'bg-white/10 text-white/50'}">2</span
								>
								Terms
							</button>
						</div>
					{/if}

					{#if currentStep === 'form'}
						<div>
							<label class="mb-1.5 block text-xs font-semibold text-white/60" for="inv-name"
								>Full Name</label
							>
							<input
								id="inv-name"
								type="text"
								bind:value={displayName}
								placeholder="Jane Doe"
								class="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white transition-all placeholder:text-white/20 focus:border-brand-500/60 focus:bg-white/8 focus:ring-2 focus:ring-brand-500/20 focus:outline-hidden"
							/>
						</div>

						<div>
							<label class="mb-1.5 block text-xs font-semibold text-white/60" for="inv-email"
								>Email Address</label
							>
							<input
								id="inv-email"
								type="email"
								bind:value={email}
								placeholder="you@example.com"
								class="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white transition-all placeholder:text-white/20 focus:border-brand-500/60 focus:bg-white/8 focus:ring-2 focus:ring-brand-500/20 focus:outline-hidden"
							/>
						</div>

						<div>
							<label class="mb-1.5 block text-xs font-semibold text-white/60" for="inv-password"
								>Password</label
							>
							<input
								id="inv-password"
								type="password"
								bind:value={password}
								placeholder="Min. 8 characters"
								class="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white transition-all placeholder:text-white/20 focus:border-brand-500/60 focus:bg-white/8 focus:ring-2 focus:ring-brand-500/20 focus:outline-hidden"
							/>
						</div>

						<div>
							<label class="mb-1.5 block text-xs font-semibold text-white/60" for="inv-confirm"
								>Confirm Password</label
							>
							<input
								id="inv-confirm"
								type="password"
								bind:value={confirmPassword}
								placeholder="Repeat your password"
								class="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white transition-all placeholder:text-white/20 focus:border-brand-500/60 focus:bg-white/8 focus:ring-2 focus:ring-brand-500/20 focus:outline-hidden"
							/>
						</div>

						{#if workflow.terms_enabled}
							<button
								type="button"
								class="w-full rounded-xl bg-brand-500 px-4 py-2.5 text-sm font-bold text-white shadow-lg shadow-brand-500/30 transition-all hover:bg-brand-600 active:scale-[0.98]"
								onclick={() => {
									const err = validateForm().replace('Please accept the Terms & Conditions.', '');
									if (err.trim()) {
										submitError = err;
										return;
									}
									submitError = '';
									currentStep = 'terms';
								}}>Continue to Terms &amp; Conditions →</button
							>
						{:else}
							{#if submitError}
								<p class="rounded-xl bg-red-500/10 px-3 py-2 text-xs font-medium text-red-400">
									{submitError}
								</p>
							{/if}
							<button
								type="button"
								class="w-full rounded-xl bg-brand-500 px-4 py-2.5 text-sm font-bold text-white shadow-lg shadow-brand-500/30 transition-all hover:bg-brand-600 active:scale-[0.98] disabled:opacity-60"
								disabled={isSubmitting}
								onclick={handleSubmit}
							>
								{isSubmitting ? 'Creating account...' : 'Create Account'}
							</button>
						{/if}
					{:else}
						<!-- Terms Step -->
						<div>
							<div
								class="mb-3 max-h-48 overflow-y-auto rounded-xl border border-white/10 bg-white/5 p-4"
							>
								<pre
									class="font-sans text-xs leading-relaxed whitespace-pre-wrap text-white/60">{workflow.terms_text ||
										''}</pre>
							</div>

							<label class="group flex cursor-pointer items-start gap-3">
								<input
									type="checkbox"
									bind:checked={termsAccepted}
									class="mt-0.5 h-4 w-4 cursor-pointer rounded border-white/20 bg-white/5 text-brand-500 focus:ring-brand-500/30"
								/>
								<span
									class="text-xs leading-relaxed text-white/60 transition-colors group-hover:text-white/80"
								>
									I have read and agree to the Terms of Service and Privacy Policy above.
								</span>
							</label>
						</div>

						{#if submitError}
							<p class="rounded-xl bg-red-500/10 px-3 py-2 text-xs font-medium text-red-400">
								{submitError}
							</p>
						{/if}

						<div class="flex gap-2">
							<button
								type="button"
								class="flex-1 rounded-xl border border-white/10 px-4 py-2.5 text-sm font-semibold text-white/60 transition-all hover:bg-white/5"
								onclick={() => (currentStep = 'form')}>← Back</button
							>
							<button
								type="button"
								class="flex-1 rounded-xl bg-brand-500 px-4 py-2.5 text-sm font-bold text-white shadow-lg shadow-brand-500/30 transition-all hover:bg-brand-600 active:scale-[0.98] disabled:opacity-60"
								disabled={isSubmitting || !termsAccepted}
								onclick={handleSubmit}
							>
								{isSubmitting ? 'Creating account...' : 'Accept & Create Account'}
							</button>
						</div>
					{/if}

					<p class="text-center text-xs text-white/30">
						Already have an account? <a
							href="/login"
							class="font-semibold text-brand-400 hover:text-brand-300">Sign in</a
						>
					</p>
				</div>
			</div>
		{:else}
			<!-- Loading skeleton -->
			<div class="animate-pulse rounded-3xl border border-white/10 bg-white/5 p-8">
				<div class="mb-4 h-8 rounded-xl bg-white/10"></div>
				<div class="mb-2 h-4 w-3/4 rounded bg-white/10"></div>
				<div class="mb-6 h-4 w-1/2 rounded bg-white/10"></div>
				<div class="space-y-3">
					<div class="h-10 rounded-xl bg-white/10"></div>
					<div class="h-10 rounded-xl bg-white/10"></div>
					<div class="h-10 rounded-xl bg-white/10"></div>
				</div>
			</div>
		{/if}
	</div>
</div>
