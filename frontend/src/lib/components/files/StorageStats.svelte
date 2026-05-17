<script lang="ts">
	import { formatFileSize } from '$lib/utils/format';

	let {
		totalFiles = 0,
		totalFolders = 0,
		totalSize = 0,
		storageQuota = 0
	}: {
		totalFiles?: number;
		totalFolders?: number;
		totalSize?: number;
		storageQuota?: number;
	} = $props();

	let usagePercent = $derived(storageQuota > 0 ? (totalSize / storageQuota) * 100 : 0);
	let usageClass = $derived(
		usagePercent >= 90
			? 'progress-error'
			: usagePercent >= 75
				? 'progress-warning'
				: 'progress-primary'
	);
</script>

<div class="stats w-full stats-vertical bg-base-100 shadow lg:stats-horizontal">
	<div class="stat">
		<div class="stat-figure text-primary">
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="h-8 w-8"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"
				/>
			</svg>
		</div>
		<div class="stat-title">Files</div>
		<div class="stat-value text-primary">{totalFiles}</div>
		<div class="stat-desc">{totalFolders} folders</div>
	</div>

	<div class="stat">
		<div class="stat-figure text-secondary">
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="h-8 w-8"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M20.25 6.375c0 2.278-3.694 4.125-8.25 4.125S3.75 8.653 3.75 6.375m16.5 0c0-2.278-3.694-4.125-8.25-4.125S3.75 4.097 3.75 6.375m16.5 0v11.25c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125V6.375m16.5 0v3.75m-16.5-3.75v3.75m16.5 0v3.75C20.25 16.153 16.556 18 12 18s-8.25-1.847-8.25-4.125v-3.75m16.5 0c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125"
				/>
			</svg>
		</div>
		<div class="stat-title">Storage Used</div>
		<div class="stat-value text-2xl text-secondary">{formatFileSize(totalSize)}</div>
		<div class="stat-desc">of {formatFileSize(storageQuota)}</div>
	</div>

	<div class="stat">
		<div class="stat-title">Usage</div>
		<div class="stat-value text-lg">{usagePercent.toFixed(1)}%</div>
		<div class="stat-desc mt-2">
			<progress class="progress {usageClass} w-full" value={usagePercent} max="100"></progress>
		</div>
	</div>
</div>
