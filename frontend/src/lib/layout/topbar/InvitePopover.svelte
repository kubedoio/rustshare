<script lang="ts">
	import { UserPlus, X } from 'lucide-svelte';
	import { createInvite } from '$lib/api/invites';

	export let enabled = false;
	export let open = false;
	export let onSend: (email: string) => Promise<void> = async () => {};

	let inviteEmail = '';
	let inviteState: 'idle' | 'done' = 'idle';
	let inviteLink = '';
	let inviteLoading = false;
	let inviteErrorMsg = '';

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
				class="absolute right-0 mt-2 w-80 origin-top-right rounded-2xl border border-base-300 bg-base-100 p-4 shadow-2xl ring-1 ring-black/5 animate-in fade-in zoom-in duration-100 z-[200]"
			>
				<div class="flex items-center justify-between mb-3">
					<div>
						<h3 class="text-sm font-bold text-base-content">Send an Invitation</h3>
						<p class="text-xs text-base-content/50 mt-0.5">Share a unique signup link</p>
					</div>
					<button type="button" class="p-1 rounded-lg hover:bg-base-200 text-base-content/40 hover:text-base-content" on:click={resetInvite}>
						<X size={16} />
					</button>
				</div>

				{#if inviteState === 'idle'}
					<div class="space-y-3">
						<div>
							<label class="text-xs font-semibold text-base-content/70 mb-1 block" for="invite-email">Recipient email</label>
							<input
								id="invite-email"
								type="email"
								bind:value={inviteEmail}
								placeholder="colleague@company.com"
								class="w-full rounded-xl border border-base-300/60 bg-base-200/50 px-3 py-2 text-sm text-base-content placeholder:text-base-content/30 focus:border-brand-500/50 focus:bg-base-100 focus:outline-none focus:ring-2 focus:ring-brand-500/10"
								on:keydown={(e) => e.key === 'Enter' && handleSendInvite()}
							/>
						</div>
						<p class="text-2xs text-base-content/40 leading-relaxed">
							This will generate a unique invite link powered by the <a href="/admin/workflows" class="text-brand-500 hover:underline">Invite Email workflow</a>.
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
							<p class="text-xs text-red-500 mt-2">{inviteErrorMsg}</p>
						{/if}
					</div>
				{:else}
					<div class="space-y-3">
						<div class="flex items-center gap-2 rounded-xl bg-success/10 border border-success/20 px-3 py-2">
							<div class="h-2 w-2 rounded-full bg-success shrink-0"></div>
							<p class="text-xs font-medium text-success-content">Invite link ready for <span class="font-bold">{inviteEmail}</span></p>
						</div>
						<div class="rounded-xl border border-base-300/50 bg-base-200/50 p-2">
							<p class="text-2xs text-base-content/50 mb-1 font-semibold uppercase tracking-wider">Invite Link</p>
							<p class="text-meta text-base-content/80 break-all font-mono leading-relaxed">{inviteLink}</p>
						</div>
						<button
							type="button"
							class="w-full rounded-xl bg-brand-500 px-4 py-2 text-sm font-bold text-white shadow-sm transition-all hover:bg-brand-600 active:scale-[0.98]"
							on:click={() => navigator.clipboard.writeText(inviteLink)}
						>Copy Link</button>
						<button type="button" class="w-full text-xs text-base-content/50 hover:text-base-content" on:click={() => { inviteState = 'idle'; inviteEmail = ''; }}>Invite someone else</button>
					</div>
				{/if}
			</div>
		{/if}
	</div>
{/if}
