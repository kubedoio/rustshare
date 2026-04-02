<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';

	interface Workflow {
		id: string;
		name: string;
		type: 'invite' | 'onboarding' | 'terms';
		subject: string;
		body: string;
		terms_enabled: boolean;
		terms_text: string;
		status: 'active' | 'draft';
	}

	const DEFAULT_INVITE_WORKFLOW: Workflow = {
		id: 'invite-email',
		name: 'Invite Email',
		type: 'invite',
		subject: "You've been invited to RustShare",
		body: `Hi {{recipient_name}},

{{sender_name}} has invited you to join RustShare — a secure file sharing platform.

Click the link below to accept your invitation and create your account:

{{invite_link}}

This invitation expires in 7 days.

Best regards,
The RustShare Team`,
		terms_enabled: true,
		terms_text: `Terms of Service

By accepting this invitation and creating an account, you agree to:

1. Use RustShare only for lawful purposes
2. Keep your credentials confidential and not share your account
3. Not upload content that infringes intellectual property rights
4. Not attempt to access other users' files or disrupt the service
5. Accept that uploaded files may be stored on distributed object storage

Privacy Policy

We collect only the minimum data necessary to operate the service: your email address, display name, and usage metadata. We do not sell your data to third parties.

By clicking "Accept & Create Account", you confirm that you have read, understood, and agree to these terms.`,
		status: 'active'
	};

	let workflows: Workflow[] = [];
	let selectedWorkflow: Workflow | null = null;
	let editingWorkflow: Workflow | null = null;
	let saveMessage = '';
	let previewMode = false;

	const TYPE_LABELS: Record<string, string> = {
		invite: 'Invite',
		onboarding: 'Onboarding',
		terms: 'Terms & Conditions'
	};
	const TYPE_COLORS: Record<string, string> = {
		invite: 'text-brand-600 bg-brand-500/10',
		onboarding: 'text-emerald-600 bg-emerald-500/10',
		terms: 'text-amber-600 bg-amber-500/10'
	};

	onMount(() => {
		if (!browser) return;
		try {
			const stored = localStorage.getItem('rs_workflows');
			if (stored) {
				workflows = JSON.parse(stored);
				if (!workflows.find(w => w.id === 'invite-email')) {
					workflows = [DEFAULT_INVITE_WORKFLOW, ...workflows];
					saveWorkflows();
				}
			} else {
				workflows = [DEFAULT_INVITE_WORKFLOW];
				saveWorkflows();
			}
		} catch {
			workflows = [DEFAULT_INVITE_WORKFLOW];
		}
	});

	function saveWorkflows() {
		if (!browser) return;
		localStorage.setItem('rs_workflows', JSON.stringify(workflows));
	}

	function selectWorkflow(wf: Workflow) {
		selectedWorkflow = wf;
		editingWorkflow = { ...wf };
		previewMode = false;
		saveMessage = '';
	}

	function handleSave() {
		if (!editingWorkflow) return;
		workflows = workflows.map(w => w.id === editingWorkflow!.id ? { ...editingWorkflow } : w);
		selectedWorkflow = { ...editingWorkflow };
		saveWorkflows();
		saveMessage = 'Saved!';
		setTimeout(() => saveMessage = '', 2500);
	}

	function getPreviewBody(wf: Workflow): string {
		return wf.body
			.replace(/{{recipient_name}}/g, 'Jane Doe')
			.replace(/{{sender_name}}/g, 'You')
			.replace(/{{invite_link}}/g, window.location.origin + '/invite/example-token-here');
	}
</script>

<svelte:head>
	<title>Workflows — Admin</title>
</svelte:head>

<div class="flex h-full gap-6">
	<!-- Workflow List -->
	<div class="w-72 shrink-0 flex flex-col gap-3">
		<div class="flex items-center justify-between mb-1">
			<h2 class="text-sm font-bold text-base-content uppercase tracking-wider">Workflows</h2>
			<span class="text-xs text-base-content/50">{workflows.length} configured</span>
		</div>

		{#each workflows as wf (wf.id)}
			<button
				type="button"
				class="w-full text-left rounded-2xl border p-3.5 transition-all hover:border-brand-500/40 hover:shadow-sm"
				class:border-brand-500={selectedWorkflow?.id === wf.id}
				class:bg-brand-500_10={selectedWorkflow?.id === wf.id}
				class:border-base-300={selectedWorkflow?.id !== wf.id}
				class:bg-base-100={selectedWorkflow?.id !== wf.id}
				on:click={() => selectWorkflow(wf)}
			>
				<div class="flex items-start justify-between gap-2">
					<div class="min-w-0">
						<p class="text-sm font-semibold text-base-content truncate">{wf.name}</p>
						<p class="text-xs text-base-content/50 mt-0.5 truncate">{wf.subject}</p>
					</div>
					<div class="flex flex-col items-end gap-1 shrink-0">
						<span class="text-[10px] font-bold px-1.5 py-0.5 rounded-md {TYPE_COLORS[wf.type] ?? 'text-base-content/60 bg-base-200'}">
							{TYPE_LABELS[wf.type] ?? wf.type}
						</span>
						<span class="text-[9px] font-semibold uppercase tracking-wider {wf.status === 'active' ? 'text-green-600' : 'text-base-content/40'}">
							{wf.status}
						</span>
					</div>
				</div>
			</button>
		{/each}

		<div class="mt-2 rounded-2xl border border-dashed border-base-300 p-3.5 text-center">
			<p class="text-xs text-base-content/40">More workflow types coming soon</p>
		</div>
	</div>

	<!-- Editor -->
	<div class="flex-1 min-w-0">
		{#if editingWorkflow}
			<div class="bg-base-100 rounded-2xl border border-base-300 shadow-sm overflow-hidden">
				<!-- Header -->
				<div class="flex items-center justify-between px-5 py-3.5 border-b border-base-300 bg-base-200/40">
					<div class="flex items-center gap-3">
						<h3 class="text-sm font-bold text-base-content">{editingWorkflow.name}</h3>
						<span class="text-[10px] font-bold px-2 py-0.5 rounded-md {TYPE_COLORS[editingWorkflow.type] ?? ''}">
							{TYPE_LABELS[editingWorkflow.type] ?? editingWorkflow.type}
						</span>
					</div>
					<div class="flex items-center gap-2">
						<button
							type="button"
							class="text-xs font-semibold px-3 py-1.5 rounded-xl border border-base-300 hover:bg-base-200 transition-colors"
							on:click={() => previewMode = !previewMode}
						>
							{previewMode ? 'Edit' : 'Preview'}
						</button>
						<button
							type="button"
							class="text-xs font-bold px-3 py-1.5 rounded-xl bg-brand-500 text-white hover:bg-brand-600 transition-colors shadow-sm"
							on:click={handleSave}
						>
							Save Changes
						</button>
						{#if saveMessage}
							<span class="text-xs font-semibold text-green-600 animate-in fade-in">{saveMessage}</span>
						{/if}
					</div>
				</div>

				{#if !previewMode}
					<!-- Edit Mode -->
					<div class="p-5 space-y-4 overflow-y-auto max-h-[calc(100vh-220px)]">
						<!-- Status toggle -->
						<div class="flex items-center justify-between py-2 px-3 rounded-xl bg-base-200/60 border border-base-300/50">
							<div>
								<p class="text-xs font-bold text-base-content">Workflow Status</p>
								<p class="text-[10px] text-base-content/50 mt-0.5">Inactive workflows won't be used for invite generation</p>
							</div>
							<select
								bind:value={editingWorkflow.status}
								class="text-xs font-semibold rounded-lg border border-base-300 bg-base-100 px-2 py-1 focus:outline-none focus:ring-2 focus:ring-brand-500/20"
							>
								<option value="active">Active</option>
								<option value="draft">Draft</option>
							</select>
						</div>

						<!-- Email Subject -->
						<div>
							<label class="block text-xs font-bold text-base-content/70 mb-1.5" for="wf-subject">
								Email Subject
							</label>
							<input
								id="wf-subject"
								type="text"
								bind:value={editingWorkflow.subject}
								class="w-full rounded-xl border border-base-300/60 bg-base-200/30 px-3 py-2 text-sm focus:border-brand-500/50 focus:bg-base-100 focus:outline-none focus:ring-2 focus:ring-brand-500/10"
							/>
						</div>

						<!-- Email Body -->
						<div>
							<label class="block text-xs font-bold text-base-content/70 mb-1.5" for="wf-body">
								Email Body
								<span class="font-normal text-base-content/40 ml-1">— Variables: <code class="font-mono">{"{{recipient_name}}"}</code> <code class="font-mono">{"{{sender_name}}"}</code> <code class="font-mono">{"{{invite_link}}"}</code></span>
							</label>
							<textarea
								id="wf-body"
								bind:value={editingWorkflow.body}
								rows="10"
								class="w-full rounded-xl border border-base-300/60 bg-base-200/30 px-3 py-2 text-sm font-mono focus:border-brand-500/50 focus:bg-base-100 focus:outline-none focus:ring-2 focus:ring-brand-500/10 resize-y"
							></textarea>
						</div>

						<!-- Terms & Conditions Toggle -->
						<div class="rounded-xl border border-base-300/60 overflow-hidden">
							<div class="flex items-center justify-between px-4 py-3 bg-base-200/40">
								<div>
									<p class="text-xs font-bold text-base-content">Require Terms &amp; Conditions Acceptance</p>
									<p class="text-[10px] text-base-content/50 mt-0.5">Users must check "I agree" before creating their account</p>
								</div>
								<button
									type="button"
									class="relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors focus:outline-none"
									class:bg-brand-500={editingWorkflow.terms_enabled}
									class:bg-base-300={!editingWorkflow.terms_enabled}
									on:click={() => { if (editingWorkflow) editingWorkflow.terms_enabled = !editingWorkflow.terms_enabled; }}
									aria-label="Toggle T&C requirement"
								>
									<span
										class="inline-block h-3.5 w-3.5 transform rounded-full bg-white shadow transition-transform"
										class:translate-x-4={editingWorkflow.terms_enabled}
										class:translate-x-1={!editingWorkflow.terms_enabled}
									></span>
								</button>
							</div>

							{#if editingWorkflow.terms_enabled}
								<div class="px-4 py-3 border-t border-base-300/50">
									<label class="block text-xs font-bold text-base-content/70 mb-1.5" for="wf-terms">
										Terms &amp; Conditions Text (shown on invite page)
									</label>
									<textarea
										id="wf-terms"
										bind:value={editingWorkflow.terms_text}
										rows="8"
										class="w-full rounded-xl border border-base-300/60 bg-base-200/30 px-3 py-2 text-xs font-mono focus:border-brand-500/50 focus:bg-base-100 focus:outline-none focus:ring-2 focus:ring-brand-500/10 resize-y"
									></textarea>
								</div>
							{/if}
						</div>
					</div>
				{:else}
					<!-- Preview Mode -->
					<div class="p-5 overflow-y-auto max-h-[calc(100vh-220px)]">
						<div class="max-w-lg mx-auto">
							<div class="rounded-2xl border border-base-300 overflow-hidden shadow-sm">
								<div class="bg-base-200/60 px-4 py-3 border-b border-base-300">
									<p class="text-xs text-base-content/50 mb-0.5">Subject</p>
									<p class="text-sm font-semibold text-base-content">{editingWorkflow.subject}</p>
								</div>
								<div class="p-5 bg-base-100">
									<pre class="text-sm text-base-content/80 whitespace-pre-wrap font-sans leading-relaxed">{getPreviewBody(editingWorkflow)}</pre>
								</div>
								{#if editingWorkflow.terms_enabled}
									<div class="px-5 py-4 bg-amber-500/5 border-t border-amber-500/20">
										<p class="text-xs font-bold text-amber-700 mb-2 uppercase tracking-wider">Terms &amp; Conditions Preview</p>
										<pre class="text-xs text-base-content/60 whitespace-pre-wrap font-sans leading-relaxed max-h-32 overflow-y-auto">{editingWorkflow.terms_text}</pre>
									</div>
								{/if}
							</div>
							<p class="text-[10px] text-center text-base-content/40 mt-3">This is how the invite email will appear to recipients</p>
						</div>
					</div>
				{/if}
			</div>
		{:else}
			<div class="flex items-center justify-center h-64 rounded-2xl border border-dashed border-base-300 bg-base-100/50">
				<div class="text-center">
					<div class="w-12 h-12 rounded-2xl bg-base-200 flex items-center justify-center mx-auto mb-3">
						<svg xmlns="http://www.w3.org/2000/svg" class="w-6 h-6 text-base-content/30" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
							<path stroke-linecap="round" stroke-linejoin="round" d="M7.5 21 3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5" />
						</svg>
					</div>
					<p class="text-sm font-medium text-base-content/50">Select a workflow to edit</p>
				</div>
			</div>
		{/if}
	</div>
</div>
