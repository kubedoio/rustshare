<script lang="ts">
	import { UserPlus, X } from 'lucide-svelte';
	import { createInvite } from '$lib/api/invites';

	let {
		enabled = false,
		open = $bindable(false),
		onSend = async () => {}
	}: {
		enabled?: boolean;
		open?: boolean;
		onSend?: (email: string) => Promise<void>;
	} = $props();

	let inviteEmail = $state('');
	let inviteState = $state<'idle' | 'done'>('idle');
	let inviteLink = $state('');
	let inviteLoading = $state(false);
	let inviteErrorMsg = $state('');

	function handleToggle() {
		open = !open;
		if (open) {
			inviteEmail = '';
			inviteState = 'idle';
			inviteLink = '';
			inviteErrorMsg = '';
		}
	}

	async function handleSendInvite() {
		if (!inviteEmail.trim()) return;
		inviteLoading = true;
		inviteErrorMsg = '';
		try {
			const res = await createInvite({
				recipient_email: inviteEmail.trim(),
				origin: window.location.origin
			});
			inviteLink = res.invite_link;
			await onSend(inviteEmail.trim());
			inviteState = 'done';
		} catch (err: any) {
			inviteErrorMsg = err?.message || 'Failed to send invite';
		} finally {
			inviteLoading = false;
		}
	}

	function resetInvite() {
		inviteEmail = '';
		inviteState = 'idle';
		inviteLink = '';
		open = false;
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			open = false;
		}
	}
</script>

<svelte:window on:keydown={handleKeydown} />

{#if enabled}
	<div class="relative">
		<button
			type="button"
			class="hidden items-center gap-2 rounded-xl border border-base-300/60 px-3 py-2 text-xs font-bold text-base-content/70 transition-all hover:bg-base-200 sm:flex"
			on:click={handleToggle}
			aria-expanded={open}
			aria-haspopup="dialog"
		>
			<UserPlus size={16} />
			<span>Invite</span>
		</button>

		{#if open}
			<div
				role="dialog"
				aria-modal="true"
				class="animate-in fade-in zoom-in absolute right-0 z-[200] mt-2 w-80 origin-top-right rounded-2xl border border-base-300 bg-base-100 p-4 shadow-2xl ring-1 ring-black/5 duration-100"
			>
				<div class="mb-3 flex items-center justify-between">
					<div>
						<h3 class="text-sm font-bold text-base-content">Send an Invitation</h3>
						<p class="mt-0.5 text-xs text-base-content/50">Share a unique signup link</p>
					</div>
					<button
						type="button"
						class="rounded-lg p-1 text-base-content/40 hover:bg-base-200 hover:text-base-content"
						on:click={resetInvite}
					>
						<X size={16} />
					</button>
				</div>

				{#if inviteState === 'idle'}
					<div class="space-y-3">
						<div>
							<label
								class="mb-1 block text-xs font-semibold text-base-content/70"
								for="invite-email">Recipient email</label
							>
							<input
								id="invite-email"
								type="email"
								bind:value={inviteEmail}
								placeholder="colleague@company.com"
								class="w-full rounded-xl border border-base-300/60 bg-base-200/50 px-3 py-2 text-sm text-base-content placeholder:text-base-content/30 focus:border-brand-500/50 focus:bg-base-100 focus:ring-2 focus:ring-brand-500/10 focus:outline-hidden"
								on:keydown={(e) => e.key === 'Enter' && handleSendInvite()}
							/>
						</div>
						<p class="text-2xs leading-relaxed text-base-content/40">
							This will generate a unique invite link powered by the <a
								href="/admin/workflows"
								class="text-brand-500 hover:underline">Invite Email workflow</a
							>.
						</p>
						<button
							type="button"
							class="w-full rounded-xl bg-brand-500 px-4 py-2 text-sm font-bold text-white shadow-sm transition-all hover:bg-brand-600 active:scale-[0.98] disabled:opacity-50"
							disabled={!inviteEmail.trim() || inviteLoading}
							on:click={handleSendInvite}
						>
							{inviteLoading ? 'Sending...' : 'Generate Invite Link'}
						</button>
						{#if inviteErrorMsg}
							<p class="mt-2 text-xs text-red-500">{inviteErrorMsg}</p>
						{/if}
					</div>
				{:else}
					<div class="space-y-3">
						<div
							class="flex items-center gap-2 rounded-xl border border-success/20 bg-success/10 px-3 py-2"
						>
							<div class="h-2 w-2 shrink-0 rounded-full bg-success"></div>
							<p class="text-xs font-medium text-success-content">
								Invite link ready for <span class="font-bold">{inviteEmail}</span>
							</p>
						</div>
						<div class="rounded-xl border border-base-300/50 bg-base-200/50 p-2">
							<p class="mb-1 text-2xs font-semibold tracking-wider text-base-content/50 uppercase">
								Invite Link
							</p>
							<p class="font-mono text-meta leading-relaxed break-all text-base-content/80">
								{inviteLink}
							</p>
						</div>
						<button
							type="button"
							class="w-full rounded-xl bg-brand-500 px-4 py-2 text-sm font-bold text-white shadow-sm transition-all hover:bg-brand-600 active:scale-[0.98]"
							on:click={() => navigator.clipboard.writeText(inviteLink)}>Copy Link</button
						>
						<button
							type="button"
							class="w-full text-xs text-base-content/50 hover:text-base-content"
							on:click={() => {
								inviteState = 'idle';
								inviteEmail = '';
							}}>Invite someone else</button
						>
					</div>
				{/if}
			</div>
		{/if}
	</div>
{/if}
