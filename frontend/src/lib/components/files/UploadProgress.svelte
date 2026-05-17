<script lang="ts">
	interface UploadTask {
		id: string;
		fileName: string;
		size: number;
		status: 'pending' | 'uploading' | 'success' | 'error';
		progress: number;
		error?: string;
		previewUrl?: string;
	}

	let {
		tasks = [],
		onClose = () => {}
	}: {
		tasks?: UploadTask[];
		onClose?: () => void;
	} = $props();

	function formatSize(bytes: number): string {
		if (bytes === 0) return '0 B';
		const k = 1024;
		const sizes = ['B', 'KB', 'MB', 'GB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
	}

	function getStatusIcon(status: UploadTask['status']) {
		switch (status) {
			case 'uploading':
				return 'loading';
			case 'success':
				return 'success';
			case 'error':
				return 'error';
			default:
				return 'pending';
		}
	}

	let hasActiveTasks = $derived(tasks.some((t) => t.status === 'uploading' || t.status === 'pending'));
	let allCompleted = $derived(
		tasks.length > 0 && tasks.every((t) => t.status === 'success' || t.status === 'error')
	);
</script>

{#if tasks.length > 0}
	<div
		class="fixed right-4 bottom-4 z-50 w-96 rounded-lg border border-base-300 bg-base-100 shadow-xl"
	>
		<!-- Header -->
		<div class="flex items-center justify-between border-b border-base-300 p-4">
			<h3 class="font-semibold">
				{#if hasActiveTasks}
					Uploading {tasks.filter((t) => t.status === 'uploading' || t.status === 'pending').length} file(s)
				{:else if allCompleted}
					Upload Complete
				{/if}
			</h3>
			<button
				class="btn btn-circle btn-ghost btn-sm"
				aria-label="Close upload progress"
				on:click={onClose}
				disabled={hasActiveTasks}
			>
				<svg
					xmlns="http://www.w3.org/2000/svg"
					fill="none"
					viewBox="0 0 24 24"
					stroke-width="1.5"
					stroke="currentColor"
					class="h-5 w-5"
				>
					<path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
				</svg>
			</button>
		</div>

		<!-- Upload tasks list -->
		<div class="max-h-96 overflow-y-auto">
			{#each tasks as task (task.id)}
				<div class="border-b border-base-300 p-4 last:border-b-0">
					<div class="flex items-start gap-3">
						<!-- Thumbnail or Status Icon -->
						<div class="flex-shrink-0">
							{#if task.previewUrl}
								<div class="relative h-12 w-12 overflow-hidden rounded bg-base-200">
									<img
										src={task.previewUrl}
										alt={task.fileName}
										class="h-full w-full object-cover"
									/>
									{#if task.status === 'uploading'}
										<div class="absolute inset-0 flex items-center justify-center bg-black/50">
											<span class="loading loading-sm loading-spinner text-white"></span>
										</div>
									{:else if task.status === 'success'}
										<div class="absolute inset-0 flex items-center justify-center bg-success/20">
											<svg
												xmlns="http://www.w3.org/2000/svg"
												fill="none"
												viewBox="0 0 24 24"
												stroke-width="2"
												stroke="currentColor"
												class="h-6 w-6 text-success"
											>
												<path
													stroke-linecap="round"
													stroke-linejoin="round"
													d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
												/>
											</svg>
										</div>
									{:else if task.status === 'error'}
										<div class="absolute inset-0 flex items-center justify-center bg-error/20">
											<svg
												xmlns="http://www.w3.org/2000/svg"
												fill="none"
												viewBox="0 0 24 24"
												stroke-width="2"
												stroke="currentColor"
												class="h-6 w-6 text-error"
											>
												<path
													stroke-linecap="round"
													stroke-linejoin="round"
													d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"
												/>
											</svg>
										</div>
									{/if}
								</div>
							{:else}
								<div class="mt-1">
									{#if task.status === 'uploading'}
										<span class="loading loading-sm loading-spinner"></span>
									{:else if task.status === 'success'}
										<svg
											xmlns="http://www.w3.org/2000/svg"
											fill="none"
											viewBox="0 0 24 24"
											stroke-width="1.5"
											stroke="currentColor"
											class="h-5 w-5 text-success"
										>
											<path
												stroke-linecap="round"
												stroke-linejoin="round"
												d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
											/>
										</svg>
									{:else if task.status === 'error'}
										<svg
											xmlns="http://www.w3.org/2000/svg"
											fill="none"
											viewBox="0 0 24 24"
											stroke-width="1.5"
											stroke="currentColor"
											class="h-5 w-5 text-error"
										>
											<path
												stroke-linecap="round"
												stroke-linejoin="round"
												d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"
											/>
										</svg>
									{:else}
										<svg
											xmlns="http://www.w3.org/2000/svg"
											fill="none"
											viewBox="0 0 24 24"
											stroke-width="1.5"
											stroke="currentColor"
											class="h-5 w-5 text-base-content/40"
										>
											<path
												stroke-linecap="round"
												stroke-linejoin="round"
												d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
											/>
										</svg>
									{/if}
								</div>
							{/if}
						</div>

						<!-- File info -->
						<div class="min-w-0 flex-1">
							<p class="truncate text-sm font-medium">{task.fileName}</p>
							<p class="mt-0.5 text-xs text-base-content/60">
								{formatSize(task.size)}
							</p>

							{#if task.status === 'uploading'}
								<progress
									class="progress mt-2 w-full progress-primary"
									value={task.progress}
									max="100"
								></progress>
							{:else if task.status === 'error' && task.error}
								<p class="mt-1 text-xs text-error">{task.error}</p>
							{/if}
						</div>
					</div>
				</div>
			{/each}
		</div>

		<!-- Footer with actions -->
		{#if allCompleted}
			<div class="bg-base-200 p-4">
				<button class="btn btn-block btn-sm" on:click={onClose}> Close </button>
			</div>
		{/if}
	</div>
{/if}
