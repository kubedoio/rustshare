<script lang="ts">
	import { Upload } from 'lucide-svelte';
	import { formatDate } from '$lib/utils/format';

	export let isShared: boolean = false;
	export let shareCount: number = 0;
	export let shareExpiresAt: string | null = null;
	export let size: 'xs' | 'sm' | 'md' = 'sm';

	const sizeClasses = {
		xs: 'w-3 h-3',
		sm: 'w-3.5 h-3.5',
		md: 'w-4 h-4'
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

	function getIconColor(): string {
		if (!shareExpiresAt) {
			return 'text-success'; // Green for non-expiring shares
		}
		
		const expiryDate = new Date(shareExpiresAt);
		const now = new Date();
		const daysUntilExpiry = (expiryDate.getTime() - now.getTime()) / (1000 * 60 * 60 * 24);
		
		if (expiryDate < now) {
			return 'text-error'; // Red for expired
		} else if (daysUntilExpiry <= 7) {
			return 'text-warning'; // Yellow/Orange for expiring soon (within 7 days)
		} else {
			return 'text-success'; // Green for healthy shares
		}
	}
</script>

{#if isShared}
	<span 
		class="inline-flex items-center justify-center {sizeClasses[size]} {getIconColor()}"
		title={getTooltipText()}
	>
		<Upload size={size === 'xs' ? 12 : size === 'sm' ? 14 : 16} />
	</span>
{/if}
