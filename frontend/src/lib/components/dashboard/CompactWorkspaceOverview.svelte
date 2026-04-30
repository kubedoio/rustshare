<script lang="ts">
	import { BarChart3, Database, Files, Share2 } from 'lucide-svelte';
	import { formatFileSize } from '$lib/utils/format';

	export let workspaceTitle: string;
	export let totalFiles = 0;
	export let sharedItems = 0;
	export let storageQuota: number | null = null;
	export let storageUsed = 0;

	$: storagePercent = storageQuota ? Math.min(100, (storageUsed / storageQuota) * 100) : 0;
</script>

<section class="overview-shell rs-surface" aria-label="Workspace overview">
	<div class="overview-title-block">
		<p class="overview-kicker">Workspace Dashboard</p>
		<h1>{workspaceTitle}</h1>
	</div>

	<div class="overview-stats">
		<article class="stat-card" aria-label="Files">
			<div class="stat-head">
				<Files size={16} />
				<span>Files</span>
			</div>
			<p class="stat-value">{totalFiles}</p>
		</article>

		<article class="stat-card" aria-label="Shared">
			<div class="stat-head">
				<Share2 size={16} />
				<span>Shared</span>
			</div>
			<p class="stat-value">{sharedItems}</p>
		</article>

		<article class="stat-card" aria-label="Limit">
			<div class="stat-head">
				<BarChart3 size={16} />
				<span>Limit</span>
			</div>
			<p class="stat-value">{storageQuota ? formatFileSize(storageQuota) : 'Unlimited'}</p>
		</article>

		<article class="storage-card" aria-label="Storage">
			<div class="stat-head">
				<Database size={16} />
				<span>Storage</span>
			</div>
			<div class="storage-values">
				<strong>{formatFileSize(storageUsed)}</strong>
				<span>{storageQuota ? `/ ${formatFileSize(storageQuota)}` : ''}</span>
			</div>
			<div
				class="storage-track"
				role="progressbar"
				aria-valuenow={storagePercent}
				aria-valuemin="0"
				aria-valuemax="100"
			>
				<div class="storage-fill" style={`width: ${storagePercent}%`}></div>
			</div>
		</article>
	</div>
</section>

<style>
	.overview-shell {
		display: grid;
		grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
		gap: 1rem;
		align-items: stretch;
		padding: 1.25rem 1.5rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 58%, transparent);
		border-radius: 1.75rem;
		min-height: 6.5rem;
	}

	.overview-title-block {
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 0.35rem;
		min-width: 0;
	}

	.overview-kicker {
		margin: 0;
		font-size: 0.72rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--brand-500);
	}

	.overview-title-block h1 {
		margin: 0;
		font-size: clamp(1.8rem, 3.2vw, 2.7rem);
		line-height: 1.05;
		font-weight: 700;
		color: var(--base-content);
		font-family: 'Fraunces', serif;
	}

	.overview-stats {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 0.9rem;
	}

	.stat-card,
	.storage-card {
		border: 1px solid color-mix(in oklab, var(--base-300) 52%, transparent);
		border-radius: 1.25rem;
		background: color-mix(in oklab, var(--base-100) 92%, white);
		padding: 1rem;
		min-width: 0;
	}

	.stat-head {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		margin-bottom: 0.65rem;
		font-size: 0.82rem;
		font-weight: 600;
		color: color-mix(in oklab, var(--base-content) 72%, transparent);
	}

	.stat-value {
		margin: 0;
		font-size: 1.25rem;
		font-weight: 700;
		color: var(--base-content);
	}

	.storage-values {
		display: flex;
		align-items: baseline;
		gap: 0.4rem;
		margin-bottom: 0.75rem;
	}

	.storage-values strong {
		font-size: 1rem;
	}

	.storage-values span {
		font-size: 0.8rem;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
	}

	.storage-track {
		height: 0.45rem;
		border-radius: 999px;
		background: color-mix(in oklab, var(--base-300) 70%, white);
		overflow: hidden;
	}

	.storage-fill {
		height: 100%;
		border-radius: inherit;
		background: linear-gradient(90deg, var(--brand-400), var(--brand-500));
	}

	@media (max-width: 1199px) {
		.overview-shell {
			grid-template-columns: 1fr;
		}

		.overview-stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}

	@media (max-width: 767px) {
		.overview-shell {
			padding: 1.1rem;
		}

		.overview-stats {
			grid-template-columns: 1fr;
		}

		.overview-title-block h1 {
			font-size: 1.7rem;
		}
	}
</style>
