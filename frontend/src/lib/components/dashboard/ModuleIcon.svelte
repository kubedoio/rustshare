<script lang="ts">
	import {
		FileText,
		LayoutDashboard,
		GitBranch,
		Share2,
		StickyNote,
		Folder,
		Columns3,
		CalendarDays,
		ClipboardList,
		Globe,
		Lock,
		Settings,
		Activity,
		Users,
		Lightbulb
	} from 'lucide-svelte';
	import { DEFAULT_MODULE_ICON } from '$lib/modules/iconRegistry';
	import PathSeparationIcon from './PathSeparationIcon.svelte';

	export let name: string;
	export let size: number = 20;
	export let strokeWidth: number = 1.75;

	const iconMap: Record<string, typeof FileText> = {
		'file-text': FileText,
		'layout-dashboard': LayoutDashboard,
		columns: Columns3,
		'git-branch': GitBranch,
		'path-separation': PathSeparationIcon as unknown as typeof FileText,
		'share-2': Share2,
		'sticky-note': StickyNote,
		'calendar-days': CalendarDays,
		'clipboard-list': ClipboardList,
		folder: Folder,
		globe: Globe,
		lock: Lock,
		settings: Settings,
		activity: Activity,
		users: Users,
		'lightbulb': Lightbulb
	};

	$: IconComponent = iconMap[name] || iconMap[DEFAULT_MODULE_ICON] || Folder;
</script>

<svelte:component this={IconComponent} {size} {strokeWidth} />
