<script lang="ts">
	import { formatDate } from '$lib/utils/format';

	export let isShared: boolean = false;
	export let shareCount: number = 0;
	export let shareExpiresAt: string | null = null;
	export let size: 'xs' | 'sm' | 'md' = 'sm';

	const sizeClasses = {
		xs: 'w-1.5 h-1.5',
		sm: 'w-2 h-2',
		md: 'w-3 h-3'
	};

	function getTooltipText(): string {
		if (!isShared) return '';
		
		let text = 'Shared';
		if (shareCount > 1) {
			text += ` (${shareCount} shares)`;
		}
		
		if (shareExpiresAt) {
			const expiryDate = new Date(shareExpiresAt);
			const now = new Date();
			const isExpired = expiryDate < now;
			
			if (isExpired) {
				text += ` - Expired ${formatDate(shareExpiresAt)}`;
			} else {
				text += ` - Expires ${formatDate(shareExpiresAt)}`;
			}
		} else {
			text += ' - Never expires';
		}
		
		return text;
	}

	function getDotColor(): string {
		if (!shareExpiresAt) {
			return 'bg-success'; // Green for non-expiring shares
		}
		
		const expiryDate = new Date(shareExpiresAt);
		const now = new Date();
		const daysUntilExpiry = (expiryDate.getTime() - now.getTime()) / (1000 * 60 * 60 * 24);
		
		if (expiryDate < now) {
			return 'bg-error'; // Red for expired
		} else if (daysUntilExpiry <= 7) {
			return 'bg-warning'; // Yellow/Orange for expiring soon (within 7 days)
		} else {
			return 'bg-success'; // Green for healthy shares
		}
	}
</script>

{#if isShared}
	<div class="tooltip tooltip-bottom" data-tip={getTooltipText()}>
		<div class="relative inline-flex items-center justify-center">
			<!-- Pulsing animation ring -->
			<span class="absolute inline-flex h-full w-full animate-ping rounded-full opacity-20 {getDotColor()}"></span>
			<!-- Solid dot -->
			<span class="relative inline-flex rounded-full {sizeClasses[size]} {getDotColor()}"></span>
		</div>
	</div>
{/if}
