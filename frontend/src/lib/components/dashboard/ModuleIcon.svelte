<script lang="ts">
	import {
		FileText,
		Users,
		Activity,
		LayoutTemplate,
		GitBranch,
		Share2,
		StickyNote,
		Folder,
		Columns3
	} from 'lucide-svelte';

	export let name: string;
	export let size: number = 20;
	export let strokeWidth: number = 1.75;

	const iconMap: Record<string, typeof FileText> = {
		'file-text': FileText,
		users: Users,
		activity: Activity,
		columns: Columns3,
		'git-branch': GitBranch,
		'share-2': Share2,
		'layout-template': LayoutTemplate,
		'sticky-note': StickyNote,
		folder: Folder
	};

	$: IconComponent = iconMap[name] || Folder;
</script>

<svelte:component this={IconComponent} {size} {strokeWidth} />
