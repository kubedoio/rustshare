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
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1.25rem;
		padding: 1.1rem 1.5rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 58%, transparent);
		border-radius: 1.75rem;
		min-height: 6rem;
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
		font-size: 0.68rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: color-mix(in oklab, var(--base-content) 58%, transparent);
	}

	.overview-title-block h1 {
		margin: 0;
		font-size: clamp(1.6rem, 2.5vw, 2.15rem);
		line-height: 1.08;
		font-weight: 700;
		color: var(--base-content);
		font-family: 'Fraunces', serif;
	}

	.overview-stats {
		display: flex;
		align-items: stretch;
		justify-content: flex-end;
		gap: 0.75rem;
		flex-wrap: wrap;
	}

	.stat-card,
	.storage-card {
		border: 1px solid color-mix(in oklab, var(--base-300) 52%, transparent);
		border-radius: 1.25rem;
		background: color-mix(in oklab, var(--base-100) 92%, white);
		padding: 0.85rem 0.95rem;
		min-width: 7.5rem;
	}

	.stat-head {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		margin-bottom: 0.45rem;
		font-size: 0.78rem;
		font-weight: 600;
		color: color-mix(in oklab, var(--base-content) 72%, transparent);
	}

	.stat-value {
		margin: 0;
		font-size: 1.05rem;
		font-weight: 700;
		color: var(--base-content);
	}

	.storage-card {
		min-width: 11.5rem;
	}

	.storage-values {
		display: flex;
		align-items: baseline;
		gap: 0.4rem;
		margin-bottom: 0.55rem;
		white-space: nowrap;
	}

	.storage-values strong {
		font-size: 0.98rem;
	}

	.storage-values span {
		font-size: 0.76rem;
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
			flex-direction: column;
			align-items: stretch;
		}

		.overview-stats {
			justify-content: flex-start;
		}
	}

	@media (max-width: 767px) {
		.overview-shell {
			padding: 1.1rem;
		}

		.overview-stats {
			display: grid;
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.overview-title-block h1 {
			font-size: 1.5rem;
		}

		.storage-card {
			grid-column: 1 / -1;
			min-width: 0;
		}
	}
</style>
