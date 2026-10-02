<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { calendarApi, type CalendarSource, type CalendarSourceKind } from '$lib/api/calendar';
	import { ApiError } from '$lib/api/types';
	import { queryClient } from '$lib/query-client';
	import { toastStore } from '$lib/stores/toast';
	import { RefreshCw, Upload } from 'lucide-svelte';

	const sourcesQuery = createQuery({
		queryKey: ['calendar-sources'],
		queryFn: () => calendarApi.listSources()
	});

	const importJobsQuery = createQuery({
		queryKey: ['calendar-import-jobs'],
		queryFn: () => calendarApi.listImportJobs(),
		refetchInterval: (query) =>
			query.state.data?.some((job) => job.status === 'pending' || job.status === 'running')
				? 3000
				: false
	});

	let importInput: HTMLInputElement | null = $state(null);
	let uploading = $state(false);
	let connecting: CalendarSourceKind | null = $state(null);
	let unconfigured: CalendarSourceKind[] = $state([]);
	let confirmDisconnectId: string | null = $state(null);
	let resyncingId: string | null = $state(null);

	const KIND_LABELS: Record<CalendarSourceKind, string> = {
		internal: 'Internal',
		ical_import: 'iCal import',
		google: 'Google',
		outlook: 'Outlook'
	};

	const STATUS_LABELS: Record<string, string> = {
		healthy: 'Healthy',
		degraded: 'Degraded',
		auth_required: 'Re-authorization required',
		rate_limited: 'Rate limited',
		paused: 'Paused',
		failed: 'Failed'
	};

	const OAUTH_ERROR_MESSAGES: Record<string, string> = {
		oauth_denied: 'Provider access was denied.',
		oauth_state: 'The connect session expired or is invalid. Try again.',
		oauth_exchange: 'The provider rejected the authorization. Try again.',
		oauth_unconfigured: 'This provider is not configured on this deployment.'
	};

	// OAuth callback redirect params: show a toast once, then strip them from
	// the URL so a refresh does not re-trigger the toast.
	function consumeOauthRedirectParams() {
		if (typeof window === 'undefined') return;
		const params = new URLSearchParams(window.location.search);
		const connected = params.get('connected');
		const error = params.get('error');
		if (connected) {
			toastStore.show(
				`Connected ${KIND_LABELS[connected as CalendarSourceKind] ?? connected} Calendar`,
				'success'
			);
		} else if (error?.startsWith('oauth_')) {
			toastStore.show(
				OAUTH_ERROR_MESSAGES[error] ?? 'Connecting the calendar provider failed.',
				'error'
			);
		} else {
			return;
		}
		params.delete('connected');
		params.delete('error');
		const query = params.toString();
		const url = window.location.pathname + (query ? `?${query}` : '') + window.location.hash;
		window.history.replaceState(null, '', url);
	}

	consumeOauthRedirectParams();

	const sources = $derived($sourcesQuery.data ?? []);
	const importJobs = $derived(($importJobsQuery.data ?? []).slice(0, 10));
	const internalSource = $derived(sources.find((source) => source.kind === 'internal') ?? null);

	function isOauthKind(kind: CalendarSourceKind): boolean {
		return kind === 'google' || kind === 'outlook';
	}

	function statusBadgeClass(status: string): string {
		switch (status) {
			case 'healthy':
				return 'badge-success';
			case 'degraded':
			case 'rate_limited':
				return 'badge-warning';
			case 'auth_required':
			case 'failed':
				return 'badge-error';
			default:
				return 'badge-ghost';
		}
	}

	async function connectProvider(kind: 'google' | 'outlook') {
		connecting = kind;
		try {
			const { authorize_url } = await calendarApi.connectSource(kind);
			window.location.assign(authorize_url);
		} catch (error) {
			if (error instanceof ApiError && error.status === 503) {
				if (!unconfigured.includes(kind)) unconfigured = [...unconfigured, kind];
				toastStore.show(
					`${KIND_LABELS[kind]} Calendar is not configured on this deployment.`,
					'info'
				);
			} else {
				toastStore.show(
					`Could not start ${KIND_LABELS[kind]} connect: ${error instanceof Error ? error.message : 'unknown error'}`,
					'error'
				);
			}
		} finally {
			connecting = null;
		}
	}

	function connectDisabledReason(kind: 'google' | 'outlook'): string | null {
		if (unconfigured.includes(kind)) {
			return `${KIND_LABELS[kind]} Calendar is not configured on this deployment.`;
		}
		return null;
	}

	async function disconnectSource(source: CalendarSource) {
		confirmDisconnectId = null;
		try {
			await calendarApi.disconnectSource(source.id);
			toastStore.show(`Disconnected ${source.display_name}`, 'success');
		} catch (error) {
			toastStore.show(
				`Disconnect failed: ${error instanceof Error ? error.message : 'unknown error'}`,
				'error'
			);
		} finally {
			await queryClient.invalidateQueries({ queryKey: ['calendar-sources'] });
		}
	}

	async function resyncSource(source: CalendarSource) {
		resyncingId = source.id;
		try {
			await calendarApi.resyncSource(source.id);
			toastStore.show(`Resync requested for ${source.display_name}`, 'success');
		} catch (error) {
			const message =
				error instanceof ApiError && error.status === 409
					? 'A sync is already running for this source.'
					: `Resync failed: ${error instanceof Error ? error.message : 'unknown error'}`;
			toastStore.show(
				message,
				error instanceof ApiError && error.status === 409 ? 'info' : 'error'
			);
		} finally {
			resyncingId = null;
		}
	}

	async function handleImportFile(event: Event) {
		const input = event.target as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		uploading = true;
		try {
			const result = await calendarApi.uploadIcs(file);
			toastStore.show(`Import queued for ${file.name} (${result.status})`, 'success');
			await queryClient.invalidateQueries({ queryKey: ['calendar-import-jobs'] });
			await queryClient.invalidateQueries({ queryKey: ['calendar-sources'] });
			await queryClient.invalidateQueries({ queryKey: ['calendar-events'] });
		} catch (error) {
			toastStore.show(
				`Import failed: ${error instanceof Error ? error.message : 'unknown error'}`,
				'error'
			);
		} finally {
			uploading = false;
		}
	}
</script>

<div>
	<div class="mb-4 space-y-1">
		<h2 class="text-lg font-semibold text-base-content">Calendar settings</h2>
		<p class="text-sm text-base-content/60">
			Manage calendar sources, import .ics files, and connect external providers.
		</p>
	</div>

	<div class="flex flex-col gap-4">
		<!-- Sources -->
		<div class="rounded-lg border border-[var(--rs-border)] bg-[var(--rs-surface-raised)]">
			<div class="border-b border-[var(--rs-border)] px-4 py-3">
				<h3 class="text-sm font-semibold text-base-content">Sources</h3>
			</div>
			{#if $sourcesQuery.isLoading}
				<div class="flex flex-col gap-1.5 p-4" aria-label="Loading sources">
					<div class="h-10 w-full skeleton"></div>
					<div class="h-10 w-full skeleton opacity-70"></div>
				</div>
			{:else if $sourcesQuery.isError}
				<p class="p-4 text-xs text-error" role="alert">Calendar sources could not be loaded.</p>
			{:else if sources.length === 0}
				<div class="flex flex-col items-center px-6 py-10 text-center">
					<p class="text-sm font-medium text-base-content">No calendar sources yet</p>
					<p class="mt-1 text-xs text-base-content/60">
						Import an .ics file below to add events to your calendar.
					</p>
				</div>
			{:else}
				<ul class="divide-y divide-[var(--rs-border)]">
					{#each sources as source}
						<li class="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-3">
							<div class="min-w-0 flex-1">
								<div class="flex flex-wrap items-center gap-2">
									<span class="truncate text-sm font-medium text-base-content">
										{source.display_name}
									</span>
									<span class="badge badge-ghost badge-sm">{KIND_LABELS[source.kind]}</span>
									<span class="badge badge-sm {statusBadgeClass(source.status)}">
										{STATUS_LABELS[source.status] ?? source.status}
									</span>
								</div>
								<p class="mt-0.5 truncate text-xs text-base-content/55">
									{#if source.external_account}
										{source.external_account}
										{#if source.last_synced_at}
											· Last synced {new Date(source.last_synced_at).toLocaleString()}
										{/if}
									{:else if source.last_synced_at}
										Last synced {new Date(source.last_synced_at).toLocaleString()}
									{:else}
										Never synced
									{/if}
								</p>
								{#if source.last_error}
									<p class="mt-0.5 truncate text-xs text-error" title={source.last_error}>
										{source.last_error}
									</p>
								{/if}
							</div>
							{#if isOauthKind(source.kind)}
								<div class="flex items-center gap-1.5">
									{#if source.status === 'auth_required'}
										<button
											type="button"
											class="btn btn-outline btn-xs"
											onclick={() => connectProvider(source.kind as 'google' | 'outlook')}
											disabled={connecting !== null}
										>
											{connecting === source.kind ? 'Connecting…' : 'Reconnect'}
										</button>
									{:else}
										<button
											type="button"
											class="btn btn-outline btn-xs"
											disabled={resyncingId === source.id}
											onclick={() => resyncSource(source)}
										>
											{#if resyncingId === source.id}
												<span class="loading loading-xs loading-spinner"></span>
											{:else}
												<RefreshCw size={12} />
											{/if}
											Resync
										</button>
									{/if}
									{#if confirmDisconnectId === source.id}
										<button
											type="button"
											class="btn btn-error btn-xs"
											onclick={() => disconnectSource(source)}
										>
											Confirm disconnect
										</button>
										<button
											type="button"
											class="btn btn-ghost btn-xs"
											onclick={() => (confirmDisconnectId = null)}
										>
											Cancel
										</button>
									{:else}
										<button
											type="button"
											class="btn btn-ghost btn-xs"
											onclick={() => (confirmDisconnectId = source.id)}
										>
											Disconnect
										</button>
									{/if}
								</div>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</div>

		<!-- Import -->
		<div class="rounded-lg border border-[var(--rs-border)] bg-[var(--rs-surface-raised)]">
			<div class="border-b border-[var(--rs-border)] px-4 py-3">
				<h3 class="text-sm font-semibold text-base-content">Import .ics file</h3>
				<p class="mt-0.5 text-xs text-base-content/55">
					Upload an iCalendar export. Events are parsed and added in the background; re-importing
					the same file updates in place.
				</p>
			</div>
			<div class="p-4">
				<input
					type="file"
					accept=".ics,text/calendar"
					class="hidden"
					aria-label="Choose .ics file"
					bind:this={importInput}
					onchange={handleImportFile}
				/>
				<button
					type="button"
					class="btn gap-1.5 btn-outline btn-sm"
					disabled={uploading}
					onclick={() => importInput?.click()}
				>
					{#if uploading}<span class="loading loading-xs loading-spinner"></span>{/if}
					<Upload size={14} /> Choose .ics file
				</button>

				{#if importJobs.length > 0}
					<ul class="mt-4 flex flex-col gap-1.5">
						{#each importJobs as job}
							<li class="flex flex-wrap items-center gap-2 text-xs text-base-content/70">
								<span class="truncate">{job.filename}</span>
								<span class="badge badge-ghost badge-sm">{job.status}</span>
								<span class="text-base-content/50">
									{job.processed_events}/{job.total_events} events
								</span>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		</div>

		<!-- External providers -->
		<div class="rounded-lg border border-[var(--rs-border)] bg-[var(--rs-surface-raised)]">
			<div class="border-b border-[var(--rs-border)] px-4 py-3">
				<h3 class="text-sm font-semibold text-base-content">Connect a provider</h3>
				<p class="mt-0.5 text-xs text-base-content/55">
					Sync events read-only from an external calendar account.
				</p>
			</div>
			<div class="flex flex-wrap gap-2 p-4">
				{#each ['google', 'outlook'] as kind}
					<button
						type="button"
						class="btn btn-outline btn-sm"
						disabled={connecting !== null || unconfigured.includes(kind as CalendarSourceKind)}
						title={connectDisabledReason(kind as 'google' | 'outlook') ??
							`Connect your ${KIND_LABELS[kind as CalendarSourceKind]} account`}
						onclick={() => connectProvider(kind as 'google' | 'outlook')}
					>
						{#if connecting === kind}<span class="loading loading-xs loading-spinner"></span>{/if}
						Connect {KIND_LABELS[kind as CalendarSourceKind]} Calendar
					</button>
				{/each}
			</div>
			{#if internalSource}
				<p class="border-t border-[var(--rs-border)] px-4 py-2 text-2xs text-base-content/50">
					Internal events live in "{internalSource.display_name}".
				</p>
			{/if}
		</div>
	</div>
</div>
