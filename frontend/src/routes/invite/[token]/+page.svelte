<script lang="ts">
	import { page } from '$app/stores';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { browser } from '$app/environment';

	interface InviteData {
		senderId: string;
		senderName: string;
		recipientEmail: string;
		timestamp: number;
	}

	interface Workflow {
		id: string;
		subject: string;
		body: string;
		terms_enabled: boolean;
		terms_text: string;
		status: string;
	}

	let token = $page.params.token;
	let inviteData: InviteData | null = null;
	let workflow: Workflow | null = null;
	let parseError = false;

	// Form state
	let displayName = '';
	let email = '';
	let password = '';
	let confirmPassword = '';
	let termsAccepted = false;
	let isSubmitting = false;
	let submitError = '';
	let submitted = false;

	// UI state
	let currentStep: 'form' | 'terms' = 'form';

	const DEFAULT_WORKFLOW: Workflow = {
		id: 'invite-email',
		subject: "You've been invited to RustShare",
		body: '',
		terms_enabled: true,
		terms_text: `Terms of Service

By accepting this invitation and creating an account, you agree to:

1. Use RustShare only for lawful purposes
2. Keep your credentials confidential and not share your account
3. Not upload content that infringes intellectual property rights
4. Not attempt to access other users' files or disrupt the service

Privacy Policy

We collect only the minimum data necessary to operate the service: your email address, display name, and usage metadata. We do not sell your data to third parties.`,
		status: 'active'
	};

	onMount(() => {
		if (!browser) return;

		// Decode invite token
		try {
			const decoded = atob(token);
			const parts = decoded.split(':');
			if (parts.length < 4) throw new Error('Invalid token');
			inviteData = {
				senderId: parts[0],
				senderName: parts[1],
				recipientEmail: parts[2],
				timestamp: parseInt(parts[3])
			};
			email = inviteData.recipientEmail;

			// Check expiry (7 days)
			const now = Date.now();
			const sevenDays = 7 * 24 * 60 * 60 * 1000;
			if (now - inviteData.timestamp > sevenDays) {
				parseError = true;
				return;
			}
		} catch {
			parseError = true;
			return;
		}

		// Load workflow config
		try {
			const stored = localStorage.getItem('rs_workflows');
			const workflows = stored ? JSON.parse(stored) : [];
			workflow = workflows.find((w: Workflow) => w.id === 'invite-email') ?? DEFAULT_WORKFLOW;
		} catch {
			workflow = DEFAULT_WORKFLOW;
		}
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
		if (err) { submitError = err; return; }

		isSubmitting = true;
		// Simulate a brief delay then redirect — wire to actual register API here
		await new Promise(r => setTimeout(r, 800));
		submitted = true;
		isSubmitting = false;
		setTimeout(() => goto('/login'), 2500);
	}
</script>

<svelte:head>
	<title>You're Invited — RustShare</title>
	<meta name="description" content="Accept your invitation to join RustShare, a secure file sharing platform." />
</svelte:head>

<div class="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
	<!-- Background decoration -->
	<div class="absolute inset-0 overflow-hidden pointer-events-none">
		<div class="absolute -top-40 -right-40 w-96 h-96 rounded-full bg-brand-500/10 blur-3xl"></div>
		<div class="absolute -bottom-40 -left-40 w-96 h-96 rounded-full bg-purple-500/10 blur-3xl"></div>
	</div>

	<div class="relative w-full max-w-md">
		{#if parseError}
			<!-- Invalid / Expired Token -->
			<div class="rounded-3xl bg-white/5 border border-white/10 backdrop-blur-xl p-8 text-center shadow-2xl">
				<div class="w-16 h-16 rounded-2xl bg-red-500/20 flex items-center justify-center mx-auto mb-4">
					<svg xmlns="http://www.w3.org/2000/svg" class="w-8 h-8 text-red-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
					</svg>
				</div>
				<h1 class="text-xl font-bold text-white mb-2">Invite Link Expired or Invalid</h1>
				<p class="text-white/50 text-sm mb-6">This invite link has expired (links are valid for 7 days) or is not valid. Please ask your contact to send a new invitation.</p>
				<a href="/login" class="inline-block text-sm font-bold text-brand-400 hover:text-brand-300 transition-colors">Back to Login →</a>
			</div>

		{:else if submitted}
			<!-- Success State -->
			<div class="rounded-3xl bg-white/5 border border-white/10 backdrop-blur-xl p-8 text-center shadow-2xl">
				<div class="w-16 h-16 rounded-2xl bg-green-500/20 flex items-center justify-center mx-auto mb-4">
					<svg xmlns="http://www.w3.org/2000/svg" class="w-8 h-8 text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
					</svg>
				</div>
				<h1 class="text-xl font-bold text-white mb-2">Welcome to RustShare!</h1>
				<p class="text-white/50 text-sm mb-3">Your account is being created. Redirecting you to login...</p>
				<div class="flex items-center justify-center gap-2">
					<div class="w-1.5 h-1.5 rounded-full bg-green-400 animate-bounce" style="animation-delay:0ms"></div>
					<div class="w-1.5 h-1.5 rounded-full bg-green-400 animate-bounce" style="animation-delay:150ms"></div>
					<div class="w-1.5 h-1.5 rounded-full bg-green-400 animate-bounce" style="animation-delay:300ms"></div>
				</div>
			</div>

		{:else if inviteData && workflow}
			<!-- Invite Form -->
			<div class="rounded-3xl bg-white/5 border border-white/10 backdrop-blur-xl shadow-2xl overflow-hidden">
				<!-- Header -->
				<div class="px-7 pt-7 pb-5 border-b border-white/10">
					<div class="flex items-center gap-3 mb-4">
						<div class="w-10 h-10 rounded-xl bg-brand-500 flex items-center justify-center shadow-lg shadow-brand-500/30">
							<svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
								<path stroke-linecap="round" stroke-linejoin="round" d="M13.19 8.688a4.5 4.5 0 011.242 7.244l-4.5 4.5a4.5 4.5 0 01-6.364-6.364l1.757-1.757m13.35-.622l1.757-1.757a4.5 4.5 0 00-6.364-6.364l-4.5 4.5a4.5 4.5 0 001.242 7.244" />
							</svg>
						</div>
						<span class="text-sm font-semibold text-white/60">RustShare</span>
					</div>
					<h1 class="text-2xl font-bold text-white leading-tight">You've been invited!</h1>
					<p class="text-white/50 text-sm mt-1.5">
						<span class="text-brand-400 font-semibold">{inviteData.senderName}</span> has invited you to join RustShare — a secure, fast file sharing platform.
					</p>
				</div>

				<!-- Form -->
				<div class="px-7 py-5 space-y-4">
					<!-- Step indicator if T&C required -->
					{#if workflow.terms_enabled}
						<div class="flex items-center gap-2 mb-1">
							<button type="button" class="flex items-center gap-1.5 text-xs font-semibold transition-colors {currentStep === 'form' ? 'text-brand-400' : 'text-white/40'}" on:click={() => currentStep = 'form'}>
								<span class="w-5 h-5 rounded-full flex items-center justify-center text-[10px] {currentStep === 'form' ? 'bg-brand-500 text-white' : 'bg-white/10 text-white/50'}">1</span>
								Your Details
							</button>
							<div class="h-px flex-1 bg-white/10"></div>
							<button type="button" class="flex items-center gap-1.5 text-xs font-semibold transition-colors {currentStep === 'terms' ? 'text-brand-400' : 'text-white/40'}">
								<span class="w-5 h-5 rounded-full flex items-center justify-center text-[10px] {currentStep === 'terms' ? 'bg-brand-500 text-white' : 'bg-white/10 text-white/50'}">2</span>
								Terms
							</button>
						</div>
					{/if}

					{#if currentStep === 'form'}
						<div>
							<label class="block text-xs font-semibold text-white/60 mb-1.5" for="inv-name">Full Name</label>
							<input
								id="inv-name"
								type="text"
								bind:value={displayName}
								placeholder="Jane Doe"
								class="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white placeholder:text-white/20 focus:border-brand-500/60 focus:bg-white/8 focus:outline-none focus:ring-2 focus:ring-brand-500/20 transition-all"
							/>
						</div>

						<div>
							<label class="block text-xs font-semibold text-white/60 mb-1.5" for="inv-email">Email Address</label>
							<input
								id="inv-email"
								type="email"
								bind:value={email}
								placeholder="you@example.com"
								class="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white placeholder:text-white/20 focus:border-brand-500/60 focus:bg-white/8 focus:outline-none focus:ring-2 focus:ring-brand-500/20 transition-all"
							/>
						</div>

						<div>
							<label class="block text-xs font-semibold text-white/60 mb-1.5" for="inv-password">Password</label>
							<input
								id="inv-password"
								type="password"
								bind:value={password}
								placeholder="Min. 8 characters"
								class="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white placeholder:text-white/20 focus:border-brand-500/60 focus:bg-white/8 focus:outline-none focus:ring-2 focus:ring-brand-500/20 transition-all"
							/>
						</div>

						<div>
							<label class="block text-xs font-semibold text-white/60 mb-1.5" for="inv-confirm">Confirm Password</label>
							<input
								id="inv-confirm"
								type="password"
								bind:value={confirmPassword}
								placeholder="Repeat your password"
								class="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white placeholder:text-white/20 focus:border-brand-500/60 focus:bg-white/8 focus:outline-none focus:ring-2 focus:ring-brand-500/20 transition-all"
							/>
						</div>

						{#if workflow.terms_enabled}
							<button
								type="button"
								class="w-full rounded-xl bg-brand-500 px-4 py-2.5 text-sm font-bold text-white shadow-lg shadow-brand-500/30 hover:bg-brand-600 active:scale-[0.98] transition-all"
								on:click={() => {
									const err = validateForm().replace('Please accept the Terms & Conditions.', '');
									if (err.trim()) { submitError = err; return; }
									submitError = '';
									currentStep = 'terms';
								}}
							>Continue to Terms &amp; Conditions →</button>
						{:else}
							{#if submitError}
								<p class="text-xs font-medium text-red-400 bg-red-500/10 rounded-xl px-3 py-2">{submitError}</p>
							{/if}
							<button
								type="button"
								class="w-full rounded-xl bg-brand-500 px-4 py-2.5 text-sm font-bold text-white shadow-lg shadow-brand-500/30 hover:bg-brand-600 active:scale-[0.98] transition-all disabled:opacity-60"
								disabled={isSubmitting}
								on:click={handleSubmit}
							>
								{isSubmitting ? 'Creating account...' : 'Create Account'}
							</button>
						{/if}

					{:else}
						<!-- Terms Step -->
						<div>
							<div class="rounded-xl border border-white/10 bg-white/5 p-4 max-h-48 overflow-y-auto mb-3">
								<pre class="text-xs text-white/60 whitespace-pre-wrap font-sans leading-relaxed">{workflow.terms_text}</pre>
							</div>

							<label class="flex items-start gap-3 cursor-pointer group">
								<input
									type="checkbox"
									bind:checked={termsAccepted}
									class="mt-0.5 h-4 w-4 rounded border-white/20 bg-white/5 text-brand-500 focus:ring-brand-500/30 cursor-pointer"
								/>
								<span class="text-xs text-white/60 leading-relaxed group-hover:text-white/80 transition-colors">
									I have read and agree to the Terms of Service and Privacy Policy above.
								</span>
							</label>
						</div>

						{#if submitError}
							<p class="text-xs font-medium text-red-400 bg-red-500/10 rounded-xl px-3 py-2">{submitError}</p>
						{/if}

						<div class="flex gap-2">
							<button
								type="button"
								class="flex-1 rounded-xl border border-white/10 px-4 py-2.5 text-sm font-semibold text-white/60 hover:bg-white/5 transition-all"
								on:click={() => currentStep = 'form'}
							>← Back</button>
							<button
								type="button"
								class="flex-1 rounded-xl bg-brand-500 px-4 py-2.5 text-sm font-bold text-white shadow-lg shadow-brand-500/30 hover:bg-brand-600 active:scale-[0.98] transition-all disabled:opacity-60"
								disabled={isSubmitting || !termsAccepted}
								on:click={handleSubmit}
							>
								{isSubmitting ? 'Creating account...' : 'Accept & Create Account'}
							</button>
						</div>
					{/if}

					<p class="text-center text-xs text-white/30">
						Already have an account? <a href="/login" class="text-brand-400 hover:text-brand-300 font-semibold">Sign in</a>
					</p>
				</div>
			</div>
		{:else}
			<!-- Loading skeleton -->
			<div class="rounded-3xl bg-white/5 border border-white/10 p-8 animate-pulse">
				<div class="h-8 bg-white/10 rounded-xl mb-4"></div>
				<div class="h-4 bg-white/10 rounded mb-2 w-3/4"></div>
				<div class="h-4 bg-white/10 rounded mb-6 w-1/2"></div>
				<div class="space-y-3">
					<div class="h-10 bg-white/10 rounded-xl"></div>
					<div class="h-10 bg-white/10 rounded-xl"></div>
					<div class="h-10 bg-white/10 rounded-xl"></div>
				</div>
			</div>
		{/if}
	</div>
</div>
