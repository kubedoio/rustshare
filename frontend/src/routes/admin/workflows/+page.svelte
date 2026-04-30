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
			workflows = workflows.map((w) => (w.id === updated.id ? updated : w));
			selectedWorkflow = updated;
			editingWorkflow = { ...updated };
			saveMessage = 'Saved!';
		} catch {
			saveMessage = 'Save failed';
		}
		setTimeout(() => (saveMessage = ''), 2500);
	}

	async function toggleStatus() {
		if (!editingWorkflow) return;
		try {
			const target = editingWorkflow.status === 'active' ? 'draft' : 'active';
			const updated =
				target === 'active'
					? await enableWorkflow(editingWorkflow.id)
					: await disableWorkflow(editingWorkflow.id);
			workflows = workflows.map((w) => (w.id === updated.id ? updated : w));
			selectedWorkflow = updated;
			editingWorkflow = { ...updated };
			saveMessage = updated.status === 'active' ? 'Enabled' : 'Disabled';
		} catch (err: any) {
			saveMessage = err?.message || 'Failed to change status';
		}
		setTimeout(() => (saveMessage = ''), 3000);
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
	<div class="flex w-72 shrink-0 flex-col gap-3">
		<div class="mb-1 flex items-center justify-between">
			<h2 class="text-sm font-bold tracking-wider text-base-content uppercase">Workflows</h2>
			<span class="text-xs text-base-content/50">{workflows.length} configured</span>
		</div>

		{#each workflows as wf (wf.id)}
			<button
				type="button"
				class="w-full rounded-2xl border p-3.5 text-left transition-all hover:border-brand-500/40 hover:shadow-sm"
				class:border-brand-500={selectedWorkflow?.id === wf.id}
				class:bg-brand-500_10={selectedWorkflow?.id === wf.id}
				class:border-base-300={selectedWorkflow?.id !== wf.id}
				class:bg-base-100={selectedWorkflow?.id !== wf.id}
				onclick={() => selectWorkflow(wf)}
			>
				<div class="flex items-start justify-between gap-2">
					<div class="min-w-0">
						<p class="truncate text-sm font-semibold text-base-content">{wf.name}</p>
						<p class="mt-0.5 truncate text-xs text-base-content/50">{wf.subject}</p>
					</div>
					<div class="flex shrink-0 flex-col items-end gap-1">
						<span
							class="rounded-md px-1.5 py-0.5 text-[10px] font-bold {TYPE_COLORS[wf.trigger_type] ??
								'bg-base-200 text-base-content/60'}"
						>
							{TYPE_LABELS[wf.trigger_type] ?? wf.trigger_type}
						</span>
						<span
							class="text-[9px] font-semibold tracking-wider uppercase {wf.status === 'active'
								? 'text-green-600'
								: 'text-base-content/40'}"
						>
							{wf.status}
						</span>
					</div>
				</div>
			</button>
		{/each}

		<div class="mt-2 flex items-center justify-between">
			<button
				type="button"
				class="rounded-xl bg-brand-500 px-3 py-1.5 text-xs font-bold text-white shadow-sm transition-colors hover:bg-brand-600"
				onclick={() => (newModalOpen = true)}
			>
				+ New
			</button>
		</div>
	</div>

	<!-- Editor -->
	<div class="min-w-0 flex-1">
		{#if editingWorkflow}
			<div class="overflow-hidden rounded-2xl border border-base-300 bg-base-100 shadow-sm">
				<!-- Header -->
				<div
					class="flex items-center justify-between border-b border-base-300 bg-base-200/40 px-5 py-3.5"
				>
					<div class="flex items-center gap-3">
						<h3 class="text-sm font-bold text-base-content">{editingWorkflow.name}</h3>
						<span
							class="rounded-md px-2 py-0.5 text-[10px] font-bold {TYPE_COLORS[
								editingWorkflow.trigger_type
							] ?? ''}"
						>
							{TYPE_LABELS[editingWorkflow.trigger_type] ?? editingWorkflow.trigger_type}
						</span>
					</div>
					<div class="flex items-center gap-2">
						<button
							type="button"
							class="rounded-xl border border-base-300 px-3 py-1.5 text-xs font-semibold transition-colors hover:bg-base-200"
							onclick={() => (previewMode = !previewMode)}
						>
							{previewMode ? 'Edit' : 'Preview'}
						</button>
						<button
							type="button"
							class="rounded-xl bg-brand-500 px-3 py-1.5 text-xs font-bold text-white shadow-sm transition-colors hover:bg-brand-600"
							onclick={handleSave}
						>
							Save Changes
						</button>
						{#if saveMessage}
							<span class="animate-in fade-in text-xs font-semibold text-green-600"
								>{saveMessage}</span
							>
						{/if}
					</div>
				</div>

				{#if !previewMode}
					<!-- Edit Mode -->
					<div class="max-h-[calc(100vh-220px)] space-y-4 overflow-y-auto p-5">
						<!-- Status toggle -->
						<div
							class="flex items-center justify-between rounded-xl border border-base-300/50 bg-base-200/60 px-3 py-2"
						>
							<div>
								<p class="text-xs font-bold text-base-content">Workflow Status</p>
								<p class="mt-0.5 text-[10px] text-base-content/50">
									Inactive workflows won't be used for invite generation
								</p>
							</div>
							<button
								type="button"
								class="rounded-xl border px-3 py-1.5 text-xs font-bold transition-colors"
								class:bg-green-500={editingWorkflow.status === 'active'}
								class:text-white={editingWorkflow.status === 'active'}
								class:border-green-500={editingWorkflow.status === 'active'}
								class:bg-base-100={editingWorkflow.status !== 'active'}
								class:text-base-content={editingWorkflow.status !== 'active'}
								class:border-base-300={editingWorkflow.status !== 'active'}
								onclick={toggleStatus}
							>
								{editingWorkflow.status === 'active' ? 'Active' : 'Draft'}
							</button>
						</div>

						<!-- Email Subject -->
						<div>
							<label class="mb-1.5 block text-xs font-bold text-base-content/70" for="wf-subject">
								Email Subject
							</label>
							<input
								id="wf-subject"
								type="text"
								bind:value={editingWorkflow.subject}
								class="w-full rounded-xl border border-base-300/60 bg-base-200/30 px-3 py-2 text-sm focus:border-brand-500/50 focus:bg-base-100 focus:ring-2 focus:ring-brand-500/10 focus:outline-hidden"
							/>
						</div>

						<!-- Email Body -->
						<div>
							<label class="mb-1.5 block text-xs font-bold text-base-content/70" for="wf-body">
								Email Body
								<span class="ml-1 font-normal text-base-content/40"
									>— Variables: <code class="font-mono">{'{{recipient_name}}'}</code>
									<code class="font-mono">{'{{sender_name}}'}</code>
									<code class="font-mono">{'{{invite_link}}'}</code></span
								>
							</label>
							<textarea
								id="wf-body"
								bind:value={editingWorkflow.body}
								rows="10"
								class="w-full resize-y rounded-xl border border-base-300/60 bg-base-200/30 px-3 py-2 font-mono text-sm focus:border-brand-500/50 focus:bg-base-100 focus:ring-2 focus:ring-brand-500/10 focus:outline-hidden"
							></textarea>
						</div>

						<!-- Terms & Conditions Toggle -->
						<div class="overflow-hidden rounded-xl border border-base-300/60">
							<div class="flex items-center justify-between bg-base-200/40 px-4 py-3">
								<div>
									<p class="text-xs font-bold text-base-content">
										Require Terms &amp; Conditions Acceptance
									</p>
									<p class="mt-0.5 text-[10px] text-base-content/50">
										Users must check "I agree" before creating their account
									</p>
								</div>
								<button
									type="button"
									class="relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors focus:outline-hidden"
									class:bg-brand-500={editingWorkflow.terms_enabled}
									class:bg-base-300={!editingWorkflow.terms_enabled}
									onclick={() => {
										if (editingWorkflow)
											editingWorkflow.terms_enabled = !editingWorkflow.terms_enabled;
									}}
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
								<div class="border-t border-base-300/50 px-4 py-3">
									<label class="mb-1.5 block text-xs font-bold text-base-content/70" for="wf-terms">
										Terms &amp; Conditions Text (shown on invite page)
									</label>
									<textarea
										id="wf-terms"
										bind:value={editingWorkflow.terms_text}
										rows="8"
										class="w-full resize-y rounded-xl border border-base-300/60 bg-base-200/30 px-3 py-2 font-mono text-xs focus:border-brand-500/50 focus:bg-base-100 focus:ring-2 focus:ring-brand-500/10 focus:outline-hidden"
									></textarea>
								</div>
							{/if}
						</div>
					</div>
				{:else}
					<!-- Preview Mode -->
					<div class="max-h-[calc(100vh-220px)] overflow-y-auto p-5">
						<div class="mx-auto max-w-lg">
							<div class="overflow-hidden rounded-2xl border border-base-300 shadow-sm">
								<div class="border-b border-base-300 bg-base-200/60 px-4 py-3">
									<p class="mb-0.5 text-xs text-base-content/50">Subject</p>
									<p class="text-sm font-semibold text-base-content">{editingWorkflow.subject}</p>
								</div>
								<div class="bg-base-100 p-5">
									<pre
										class="font-sans text-sm leading-relaxed whitespace-pre-wrap text-base-content/80">{getPreviewBody(
											editingWorkflow
										)}</pre>
								</div>
								{#if editingWorkflow.terms_enabled}
									<div class="border-t border-amber-500/20 bg-amber-500/5 px-5 py-4">
										<p class="mb-2 text-xs font-bold tracking-wider text-amber-700 uppercase">
											Terms &amp; Conditions Preview
										</p>
										<pre
											class="max-h-32 overflow-y-auto font-sans text-xs leading-relaxed whitespace-pre-wrap text-base-content/60">{editingWorkflow.terms_text}</pre>
									</div>
								{/if}
							</div>
							<p class="mt-3 text-center text-[10px] text-base-content/40">
								This is how the invite email will appear to recipients
							</p>
						</div>
					</div>
				{/if}
			</div>
		{:else}
			<div
				class="flex h-64 items-center justify-center rounded-2xl border border-dashed border-base-300 bg-base-100/50"
			>
				<div class="text-center">
					<div
						class="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-2xl bg-base-200"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							class="h-6 w-6 text-base-content/30"
							fill="none"
							viewBox="0 0 24 24"
							stroke="currentColor"
							stroke-width="1.5"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								d="M7.5 21 3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5"
							/>
						</svg>
					</div>
					<p class="text-sm font-medium text-base-content/50">Select a workflow to edit</p>
				</div>
			</div>
		{/if}
	</div>
</div>

{#if newModalOpen}
	<div
		class="fixed inset-0 z-[200] flex items-center justify-center bg-black/40 backdrop-blur-sm"
		role="presentation"
		tabindex="-1"
		onclick={() => (newModalOpen = false)}
		onkeydown={(e) => {
			if (e.key === 'Escape') newModalOpen = false;
		}}
	>
		<div
			class="w-80 rounded-2xl border border-base-300 bg-base-100 p-6 shadow-xl"
			role="dialog"
			tabindex="-1"
			onclick={(e) => e.stopPropagation()}
			onkeydown={(e) => e.stopPropagation()}
		>
			<h3 class="mb-2 text-sm font-bold text-base-content">New Workflow</h3>
			<p class="mb-4 text-xs text-base-content/60">More workflow types coming soon.</p>
			<div class="flex justify-end gap-2">
				<button
					type="button"
					class="rounded-xl border border-base-300 px-3 py-1.5 text-xs font-semibold transition-colors hover:bg-base-200"
					onclick={() => (newModalOpen = false)}
				>
					Close
				</button>
				<button
					type="button"
					class="cursor-not-allowed rounded-xl bg-brand-500 px-3 py-1.5 text-xs font-bold text-white opacity-50"
					disabled
				>
					Create
				</button>
			</div>
		</div>
	</div>
{/if}
