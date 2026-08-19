<script lang="ts">
	import { Hash, Lock, Sparkles } from 'lucide-svelte';

	interface Props {
		channelId: string;
		channelName: string;
		channelKind?: string | null;
		visibility?: string | null;
		member?: boolean | null;
		askAvailable: boolean;
		askHref?: string;
		communityId?: string;
	}

	let {
		channelId,
		channelName,
		channelKind = null,
		visibility = null,
		member = null,
		askAvailable,
		askHref = '',
		communityId = ''
	}: Props = $props();

	function askHrefWithChannel(): string {
		if (askHref) return askHref;
		if (!communityId || !channelId) return '#';
		const params = new URLSearchParams({
			scope: 'chat',
			communityId,
			channelId
		});
		return `/ask?${params.toString()}`;
	}
</script>

<header
	class="flex shrink-0 items-center justify-between border-b border-base-300 bg-base-100 px-4 py-3"
>
	<div class="min-w-0">
		<h1 class="flex items-center gap-2 text-lg font-semibold text-base-content">
			<Hash size={18} class="shrink-0 text-base-content/40" />
			<span class="truncate" title={channelName}>{channelName}</span>
		</h1>
		<div class="mt-0.5 flex items-center gap-2 text-xs text-base-content/60">
			{#if visibility === 'private'}
				<span class="inline-flex items-center gap-1" title="Private channel">
					<Lock size={10} />
					Private
				</span>
			{/if}
			{#if channelKind}
				<span class="capitalize">{channelKind.replace(/_/g, ' ')}</span>
			{/if}
			{#if member === false}
				<span>Not a member</span>
			{/if}
		</div>
	</div>

	{#if askAvailable}
		<a
			href={askHrefWithChannel()}
			class="btn btn-sm btn-primary inline-flex shrink-0 items-center gap-1.5"
		>
			<Sparkles size={14} />
			Ask Elembra
		</a>
	{:else}
		<span
			class="tooltip tooltip-left inline-flex shrink-0"
			data-tip="Ask Elembra is unavailable right now"
		>
			<button
				type="button"
				class="btn btn-sm btn-primary inline-flex items-center gap-1.5"
				disabled
			>
				<Sparkles size={14} />
				Ask Elembra
			</button>
		</span>
	{/if}
</header>
