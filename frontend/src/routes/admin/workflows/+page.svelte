<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import {
		listWorkflows,
		updateWorkflow,
		enableWorkflow,
		disableWorkflow,
		type Workflow
	} from '$lib/api/workflows';

	let workflows: Workflow[] = [];
	let selectedWorkflow: Workflow | null = null;
	let editingWorkflow: Workflow | null = null;
	let saveMessage = '';
	let previewMode = false;
	let loading = false;
	let newModalOpen = false;

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
		void (async () => {
			if (!browser) return;
			loading = true;
			try {
				workflows = await listWorkflows();
				if (workflows.length > 0 && !selectedWorkflow) {
					selectWorkflow(workflows[0]);
				}
			} catch (e) {
				workflows = [];
			} finally {
				loading = false;
			}
		})();
	});

	function selectWorkflow(wf: Workflow) {
		selectedWorkflow = wf;
		editingWorkflow = { ...wf };
		previewMode = false;
		saveMessage = '';
	}

	async function handleSave() {
		if (!editingWorkflow) return;
		try {
			const updated = await updateWorkflow(editingWorkflow.id, {
				subject: editingWorkflow.subject,
				body: editingWorkflow.body,
				terms_enabled: editingWorkflow.terms_enabled,
				terms_text: editingWorkflow.terms_text
			});
			workflows = workflows.map(w => w.id === updated.id ? updated : w);
			selectedWorkflow = updated;
			editingWorkflow = { ...updated };
			saveMessage = 'Saved!';
		} catch {
			saveMessage = 'Save failed';
		}
		setTimeout(() => saveMessage = '', 2500);
	}

	async function toggleStatus() {
		if (!editingWorkflow) return;
		try {
			const target = editingWorkflow.status === 'active' ? 'draft' : 'active';
			const updated = target === 'active'
				? await enableWorkflow(editingWorkflow.id)
				: await disableWorkflow(editingWorkflow.id);
			workflows = workflows.map(w => w.id === updated.id ? updated : w);
			selectedWorkflow = updated;
			editingWorkflow = { ...updated };
			saveMessage = updated.status === 'active' ? 'Enabled' : 'Disabled';
		} catch (err: any) {
			saveMessage = err?.message || 'Failed to change status';
		}
		setTimeout(() => saveMessage = '', 3000);
	}

	function getPreviewBody(wf: Workflow): string {
		return (wf.body || '')
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
						<span class="text-[10px] font-bold px-1.5 py-0.5 rounded-md {TYPE_COLORS[wf.trigger_type] ?? 'text-base-content/60 bg-base-200'}">
							{TYPE_LABELS[wf.trigger_type] ?? wf.trigger_type}
						</span>
						<span class="text-[9px] font-semibold uppercase tracking-wider {wf.status === 'active' ? 'text-green-600' : 'text-base-content/40'}">
							{wf.status}
						</span>
					</div>
				</div>
			</button>
		{/each}

		<div class="mt-2 flex items-center justify-between">
			<button
				type="button"
				class="text-xs font-bold px-3 py-1.5 rounded-xl bg-brand-500 text-white hover:bg-brand-600 transition-colors shadow-sm"
				on:click={() => newModalOpen = true}
			>
				+ New
			</button>
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
						<span class="text-[10px] font-bold px-2 py-0.5 rounded-md {TYPE_COLORS[editingWorkflow.trigger_type] ?? ''}">
							{TYPE_LABELS[editingWorkflow.trigger_type] ?? editingWorkflow.trigger_type}
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
							<button
								type="button"
								class="text-xs font-bold px-3 py-1.5 rounded-xl border transition-colors"
								class:bg-green-500={editingWorkflow.status === 'active'}
								class:text-white={editingWorkflow.status === 'active'}
								class:border-green-500={editingWorkflow.status === 'active'}
								class:bg-base-100={editingWorkflow.status !== 'active'}
								class:text-base-content={editingWorkflow.status !== 'active'}
								class:border-base-300={editingWorkflow.status !== 'active'}
								on:click={toggleStatus}
							>
								{editingWorkflow.status === 'active' ? 'Active' : 'Draft'}
							</button>
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
								class="w-full rounded-xl border border-base-300/60 bg-base-200/30 px-3 py-2 text-sm focus:border-brand-500/50 focus:bg-base-100 focus:outline-hidden focus:ring-2 focus:ring-brand-500/10"
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
								class="w-full rounded-xl border border-base-300/60 bg-base-200/30 px-3 py-2 text-sm font-mono focus:border-brand-500/50 focus:bg-base-100 focus:outline-hidden focus:ring-2 focus:ring-brand-500/10 resize-y"
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
									class="relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors focus:outline-hidden"
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
										class="w-full rounded-xl border border-base-300/60 bg-base-200/30 px-3 py-2 text-xs font-mono focus:border-brand-500/50 focus:bg-base-100 focus:outline-hidden focus:ring-2 focus:ring-brand-500/10 resize-y"
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

{#if newModalOpen}
<div class="fixed inset-0 z-[200] flex items-center justify-center bg-black/40 backdrop-blur-sm" on:click={() => newModalOpen = false}>
	<div class="bg-base-100 rounded-2xl border border-base-300 shadow-xl p-6 w-80" on:click|stopPropagation>
		<h3 class="text-sm font-bold text-base-content mb-2">New Workflow</h3>
		<p class="text-xs text-base-content/60 mb-4">More workflow types coming soon.</p>
		<div class="flex justify-end gap-2">
			<button
				type="button"
				class="text-xs font-semibold px-3 py-1.5 rounded-xl border border-base-300 hover:bg-base-200 transition-colors"
				on:click={() => newModalOpen = false}
			>
				Close
			</button>
			<button
				type="button"
				class="text-xs font-bold px-3 py-1.5 rounded-xl bg-brand-500 text-white opacity-50 cursor-not-allowed"
				disabled
			>
				Create
			</button>
		</div>
	</div>
</div>
{/if}
