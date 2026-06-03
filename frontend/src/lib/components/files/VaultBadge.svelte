<script lang="ts">
	import { CloudUpload } from 'lucide-svelte';
	import { formatDate } from '$lib/utils/format';

	let {
		adapter,
		vaultName,
		serverRev,
		lastSyncedAt
	}: {
		adapter: string | undefined;
		vaultName: string | undefined;
		serverRev: number | undefined;
		lastSyncedAt: string | undefined;
	} = $props();

	function getTooltipText(): string {
		if (!adapter) return '';
		let text = `Vault Sync / ${vaultName || 'Unknown'}`;
		if (typeof serverRev === 'number') {
			text += ` / rev ${serverRev}`;
		}
		if (lastSyncedAt) {
			text += ` / Last synced ${formatDate(lastSyncedAt)}`;
		}
		return text;
	}
</script>

{#if adapter}
	<span
		class="badge badge-sm badge-ghost inline-flex items-center gap-1 whitespace-nowrap"
		title={getTooltipText()}
	>
		<CloudUpload size={12} class="flex-shrink-0" />
		<span>Vault Sync</span>
	</span>
{/if}
