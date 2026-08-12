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
		Lightbulb,
		Mail,
		MessageCircle
	} from 'lucide-svelte';
	import { DEFAULT_MODULE_ICON } from '$lib/applications/iconRegistry';
	import PathSeparationIcon from './PathSeparationIcon.svelte';

	let {
		name,
		size = 20,
		strokeWidth = 1.75
	}: {
		name: string;
		size?: number;
		strokeWidth?: number;
	} = $props();

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
		lightbulb: Lightbulb,
		mail: Mail,
		'message-circle': MessageCircle
	};

	let IconComponent = $derived(iconMap[name] || iconMap[DEFAULT_MODULE_ICON] || Folder);
</script>

<svelte:component this={IconComponent} {size} {strokeWidth} />
