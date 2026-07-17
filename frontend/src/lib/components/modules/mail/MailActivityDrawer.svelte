<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { mailApi, type MailFolder, type MailImportJob } from '$lib/api/mail';
	import MailArchivePanel from './MailArchivePanel.svelte';
	import { X } from 'lucide-svelte';

	let {
		open,
		accountId,
		accountName,
		folders,
		defaultFolder = null,
		onClose
	}: {
		open: boolean;
		accountId: string | null;
		accountName: string;
		folders: MailFolder[];
		defaultFolder?: string | null;
		onClose: () => void;
	} = $props();

	const importJobsQuery = createQuery<MailImportJob[]>({
		queryKey: ['mail-import-jobs'],
		queryFn: () => mailApi.listImportJobs(),
		refetchInterval: () => (open ? 3000 : false)
	});

	function jobProgress(job: MailImportJob): string {
		return `${job.processed_messages}/${job.total_messages} processed`;
	}

	let dialogEl: HTMLDivElement | null = $state(null);

	$effect(() => {
		if (open) {
			// Move focus into the dialog for keyboard/screen-reader users
			setTimeout(() => dialogEl?.querySelector<HTMLElement>('button')?.focus(), 0);
		}
	});

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') onClose();
	}
</script>

{#if open}
	<div
		class="modal modal-open"
		role="dialog"
		aria-modal="true"
		aria-label="Mail archive activity"
		tabindex="-1"
		onkeydown={handleKeydown}
	>
		<div class="modal-box max-w-2xl rounded-lg" bind:this={dialogEl}>
			<div class="mb-4 flex items-center justify-between gap-3">
				<div class="min-w-0">
					<h2 class="text-base font-semibold">Archive activity</h2>
					<p class="truncate text-xs text-base-content/55">{accountName}</p>
				</div>
				<button
					type="button"
					class="btn btn-ghost btn-sm btn-square"
					aria-label="Close archive activity"
					onclick={onClose}
				>
					<X size={16} />
				</button>
			</div>

			{#if accountId}
				<MailArchivePanel {accountId} {folders} {defaultFolder} />
			{:else}
				<p class="text-sm text-base-content/60">Select an account to manage archiving.</p>
			{/if}

			{#if ($importJobsQuery.data ?? []).length > 0}
				<div class="mt-4 border-t border-[var(--rs-border)] pt-3">
					<h4 class="mb-2 text-xs font-semibold text-base-content/70">Recent imports</h4>
					<div class="flex max-h-48 flex-col gap-1.5 overflow-y-auto">
						{#each ($importJobsQuery.data ?? []).slice(0, 10) as job}
							<div class="rounded-md border border-[var(--rs-border)] px-2.5 py-2">
								<div class="truncate text-xs font-medium">{job.folder_name}</div>
								<div class="text-2xs text-base-content/55">
									{job.status} · {jobProgress(job)} · failed {job.failed_messages}
								</div>
								{#if job.last_error}
									<p class="mt-1 truncate text-2xs text-error" title={job.last_error}>
										{job.last_error}
									</p>
								{/if}
							</div>
						{/each}
					</div>
				</div>
			{/if}
		</div>
		<button
			class="modal-backdrop"
			type="button"
			aria-label="Close archive activity"
			onclick={onClose}
		>
			Close
		</button>
	</div>
{/if}
