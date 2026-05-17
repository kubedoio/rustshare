<script lang="ts">
	import { Link2 } from 'lucide-svelte';
	import { formatDate } from '$lib/utils/format';

	let {
		isShared = false,
		shareCount = 0,
		shareExpiresAt = null,
		size = 'sm',
		showText = true
	}: {
		isShared?: boolean;
		shareCount?: number;
		shareExpiresAt?: string | null;
		size?: 'xs' | 'sm' | 'md';
		showText?: boolean;
	} = $props();

	const sizeClasses = {
		xs: 'text-[10px] gap-0.5',
		sm: 'text-xs gap-1',
		md: 'text-sm gap-1.5'
	};

	const iconSizes = {
		xs: 10,
		sm: 12,
		md: 14
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

	function getStatusColor(): string {
		if (!shareExpiresAt) {
			return 'text-success/70'; // Muted green for non-expiring shares
		}

		const expiryDate = new Date(shareExpiresAt);
		const now = new Date();
		const daysUntilExpiry = (expiryDate.getTime() - now.getTime()) / (1000 * 60 * 60 * 24);

		if (expiryDate < now) {
			return 'text-error/70'; // Muted red for expired
		} else if (daysUntilExpiry <= 7) {
			return 'text-warning/70'; // Muted yellow/orange for expiring soon
		} else {
			return 'text-success/70'; // Muted green for healthy shares
		}
	}

	function formatExpiryShort(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const isExpired = date < now;

		// Format: "Oct 24" or "Expired"
		if (isExpired) {
			return 'Expired';
		}

		const month = date.toLocaleDateString('en-US', { month: 'short' });
		const day = date.getDate();
		return `Exp. ${month} ${day}`;
	}
</script>

{#if isShared}
	<span
		class="inline-flex items-center {sizeClasses[size]} {getStatusColor()} whitespace-nowrap"
		title={getTooltipText()}
	>
		<Link2 size={iconSizes[size]} class="flex-shrink-0" />
		{#if showText && shareExpiresAt}
			<span class="font-medium">{formatExpiryShort(shareExpiresAt)}</span>
		{/if}
	</span>
{/if}
