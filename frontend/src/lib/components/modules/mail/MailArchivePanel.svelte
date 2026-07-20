<script lang="ts">
	import { createMutation, createQuery } from '$lib/query-compat';
	import { mailApi, type MailArchiveJob, type MailFolder, type MailImportJob } from '$lib/api/mail';
	import { toastStore } from '$lib/stores/toast';
	import { Archive, RefreshCw } from 'lucide-svelte';

	let {
		accountId,
		folders,
		defaultFolder = null
	}: {
		accountId: string | null;
		folders: MailFolder[];
		defaultFolder?: string | null;
	} = $props();

	let archiveFolderName = $state('');
	let archiveSince = $state('');
	let archiveBefore = $state('');
	let retentionDays = $state('');

	$effect(() => {
		if (!folders.some((folder) => folder.name === archiveFolderName)) {
			archiveFolderName =
				(defaultFolder && folders.some((folder) => folder.name === defaultFolder)
					? defaultFolder
					: folders[0]?.name) ?? '';
		}
	});

	const archiveJobsQuery = createQuery<MailArchiveJob[]>({
		queryKey: ['mail-archive-jobs', null],
		queryFn: () => Promise.resolve([]),
		enabled: false
	});

	$effect(() => {
		archiveJobsQuery.setOptions({
			queryKey: ['mail-archive-jobs', accountId],
			queryFn: () => mailApi.listArchiveJobs(accountId!),
			enabled: !!accountId,
			refetchInterval: 5000
		});
	});

	const archiveMutation = createMutation({
		mutationFn: () =>
			mailApi.createArchiveJob(accountId!, {
				folder_name: archiveFolderName,
				archive_since: archiveSince || null,
				archive_before: archiveBefore || null,
				retention_days: retentionDays ? Number(retentionDays) : null,
				max_retries: 5
			}),
		onSuccess: async () => {
			archiveSince = '';
			archiveBefore = '';
			retentionDays = '';
			await $archiveJobsQuery.refetch();
			toastStore.show('Archive job queued', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Archive failed', 'error')
	});

	const cancelArchiveMutation = createMutation({
		mutationFn: (jobId: string) => mailApi.cancelArchiveJob(jobId),
		onSuccess: async () => {
			await $archiveJobsQuery.refetch();
			toastStore.show('Archive job cancelled', 'success');
		},
		onError: (error) =>
			toastStore.show(error instanceof Error ? error.message : 'Cancel failed', 'error')
	});

	function jobProgress(job: MailArchiveJob | MailImportJob): string {
		return `${job.processed_messages}/${job.total_messages} processed`;
	}
</script>

<div class="flex flex-col gap-3">
	<form
		class="grid grid-cols-1 gap-2 sm:grid-cols-2"
		onsubmit={(event) => {
			event.preventDefault();
			archiveMutation.mutate();
		}}
	>
		<div class="form-control sm:col-span-2">
			<label class="label py-0.5 text-xs font-semibold" for="archive-folder">Folder</label>
			<select
				id="archive-folder"
				class="select select-sm select-bordered"
				bind:value={archiveFolderName}
				required
			>
				{#each folders as folder}
					<option value={folder.name}>{folder.display_name}</option>
				{/each}
			</select>
		</div>
		<div class="form-control">
			<label class="label py-0.5 text-xs font-semibold" for="archive-since">Archive since</label>
			<input
				id="archive-since"
				class="input input-sm input-bordered"
				type="date"
				bind:value={archiveSince}
			/>
		</div>
		<div class="form-control">
			<label class="label py-0.5 text-xs font-semibold" for="archive-before">Archive before</label>
			<input
				id="archive-before"
				class="input input-sm input-bordered"
				type="date"
				bind:value={archiveBefore}
			/>
		</div>
		<div class="form-control">
			<label class="label py-0.5 text-xs font-semibold" for="archive-retention">
				Retention (days)
			</label>
			<input
				id="archive-retention"
				class="input input-sm input-bordered"
				type="number"
				min="1"
				max="36500"
				placeholder="Keep forever"
				bind:value={retentionDays}
			/>
		</div>
		<div class="flex items-end justify-end">
			<button
				type="submit"
				class="btn btn-sm btn-outline gap-1.5"
				disabled={!accountId || !archiveFolderName || $archiveMutation.isPending}
			>
				<Archive size={13} />
				{$archiveMutation.isPending ? 'Queuing…' : 'Queue archive'}
			</button>
		</div>
	</form>

	<div class="border-t border-[var(--rs-border)] pt-3">
		<div class="mb-2 flex items-center justify-between">
			<h4 class="text-xs font-semibold text-base-content/70">Archive jobs</h4>
			<button
				type="button"
				class="btn btn-xs btn-ghost gap-1"
				onclick={() => $archiveJobsQuery.refetch()}
			>
				<RefreshCw size={11} /> Refresh
			</button>
		</div>
		{#if $archiveJobsQuery.isLoading}
			<p class="text-xs text-base-content/50">Loading archive jobs…</p>
		{:else if $archiveJobsQuery.isError}
			<p class="text-xs text-error" role="alert">
				{$archiveJobsQuery.error?.message || 'Failed to load archive jobs'}
			</p>
		{:else if ($archiveJobsQuery.data ?? []).length === 0}
			<p class="text-xs text-base-content/50">No archive jobs for this account.</p>
		{:else}
			<div class="flex max-h-64 flex-col gap-1.5 overflow-y-auto">
				{#each $archiveJobsQuery.data ?? [] as job}
					<div class="rounded-md border border-[var(--rs-border)] px-2.5 py-2">
						<div class="flex items-center justify-between gap-2">
							<div class="min-w-0">
								<div class="truncate text-xs font-medium">{job.folder_name}</div>
								<div class="text-2xs text-base-content/55">
									{job.status} · {jobProgress(job)} · retries {job.retry_count}/{job.max_retries}
								</div>
							</div>
							{#if ['pending', 'running'].includes(job.status)}
								<button
									type="button"
									class="btn btn-xs btn-outline"
									onclick={() => cancelArchiveMutation.mutate(job.id)}
								>
									Cancel
								</button>
							{/if}
						</div>
						{#if job.last_error}
							<p class="mt-1 truncate text-2xs text-error" title={job.last_error}>
								{job.last_error}
							</p>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	</div>
</div>
