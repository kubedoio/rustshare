<script lang="ts">
	import { page } from '$app/stores';
	import AskExperience from '$lib/components/ask/AskExperience.svelte';
	import type { AskScope } from '$lib/api/ask';

	let scope = $derived.by((): AskScope => {
		const type = $page.url.searchParams.get('scope');
		if (type === 'folder' || type === 'note') {
			const resourceRef = $page.url.searchParams.get('resourceRef');
			if (resourceRef) return { type, resourceRef };
		}
		if (type === 'chat') {
			const communityId = $page.url.searchParams.get('communityId');
			const channelId = $page.url.searchParams.get('channelId');
			if (communityId && channelId) return { type: 'chatChannel', communityId, channelId };
		}
		return { type: 'workspace' };
	});

	let scopeLabel = $derived(
		scope.type === 'folder'
			? 'Folder'
			: scope.type === 'note'
				? 'Current note'
				: scope.type === 'chatChannel'
					? 'Chat channel'
					: 'Workspace'
	);
</script>

<AskExperience {scope} {scopeLabel} />
