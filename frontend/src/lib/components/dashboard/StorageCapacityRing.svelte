<script lang="ts">
	import { formatFileSize } from '$lib/utils/format';

	export let used: number;
	export let quota: number | null;

	$: percent = quota ? Math.min(100, Math.max(0, (used / quota) * 100)) : 0;
	$: usedText = formatFileSize(used);
	$: quotaText = quota ? formatFileSize(quota) : 'Unlimited';
	$: label = quota ? `Storage usage: ${Math.round(percent)}% of ${quotaText} used` : 'Storage: unlimited';

	// Ring geometry
	const size = 96;
	const strokeWidth = 10;
	const radius = (size - strokeWidth) / 2;
	const circumference = 2 * Math.PI * radius;
	$: dashOffset = circumference - (percent / 100) * circumference;

	// Color by usage level
	$: ringColor =
		percent > 85
			? 'var(--rs-error, #b63e3e)'
			: percent > 60
				? 'var(--rs-warning, #a56a12)'
					: 'var(--brand-500, #c65a1e)';

	// Animate on mount
	let mounted = false;
	$: if (typeof window !== 'undefined') {
		requestAnimationFrame(() => {
			mounted = true;
		});
	}
</script>

<div class="capacity-ring" role="img" aria-label={label} title={label}>
	<svg width={size} height={size} viewBox="0 0 {size} {size}">
		<!-- Background track -->
		<circle
			cx={size / 2}
			cy={size / 2}
			r={radius}
			fill="none"
			stroke="color-mix(in oklab, var(--base-300) 60%, transparent)"
			stroke-width={strokeWidth}
		/>
		<!-- Fill arc -->
		<circle
			class="ring-fill"
			cx={size / 2}
			cy={size / 2}
			r={radius}
			fill="none"
			stroke={ringColor}
			stroke-width={strokeWidth}
			stroke-linecap="round"
			stroke-dasharray={circumference}
			stroke-dashoffset={mounted ? dashOffset : circumference}
			transform="rotate(-90 {size / 2} {size / 2})"
		/>
	</svg>

	<div class="ring-label">
		<span class="ring-percent">{Math.round(percent)}%</span>
		<span class="ring-detail">{usedText}</span>
	</div>
</div>

<style>
	.capacity-ring {
		position: relative;
		display: inline-flex;
		align-items: center;
		gap: 0.6rem;
	}

	.ring-fill {
		transition: stroke-dashoffset 800ms cubic-bezier(0.22, 1, 0.36, 1),
			stroke 300ms ease;
	}

	.ring-label {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.1rem;
	}

	.ring-percent {
		font-size: 1.1rem;
		font-weight: 800;
		line-height: 1;
		color: var(--base-content);
	}

	.ring-detail {
		font-size: 0.72rem;
		font-weight: 600;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
		white-space: nowrap;
	}

	@media (max-width: 767px) {
		.capacity-ring {
			flex-direction: column;
			align-items: center;
			gap: 0.35rem;
		}

		.ring-label {
			align-items: center;
		}
	}
</style>
