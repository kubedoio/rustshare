import {
	StickyNote,
	CalendarDays,
	GitBranch,
	Columns,
	PenTool,
	Share2,
	FileText,
	CheckCircle2,
	Lightbulb
} from 'lucide-svelte';

export function formatBytes(bytes: number): string {
	if (!bytes || bytes === 0) return '0 B';
	const k = 1024;
	const sizes = ['B', 'KB', 'MB', 'GB'];
	const i = Math.floor(Math.log(bytes) / Math.log(k));
	return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export function getArtifactTypeLabel(moduleKey: string, itemType: string): string {
	const map: Record<string, string> = {
		notes: 'Note',
		meetings: 'Meeting Note',
		standups: 'Standup',
		kanban: 'Kanban Board',
		decisions: 'Decision',
		brainstorming: 'Idea Board',
		shares: 'Share'
	};
	return map[moduleKey] ?? (itemType === 'folder' ? 'Folder' : 'File');
}

export function getArtifactHref(item: { moduleKey: string; item_type: string; id: string; name?: string }): string {
	// Folders always route to the file browser regardless of module context
	if (item.item_type === 'folder') {
		return `/files?folder=${item.id}`;
	}
	if (item.moduleKey === 'notes' && item.item_type === 'file') {
		return `/modules/notes/${item.id}`;
	}
	if (item.moduleKey === 'meetings') {
		return `/modules/meetings/${item.id}`;
	}
	if (item.moduleKey === 'standups') {
		return `/modules/standups/${item.id}`;
	}
	if (item.moduleKey === 'decisions') {
		return `/modules/decisions/${item.id}`;
	}
	if (item.moduleKey === 'brainstorming') {
		return `/modules/brainstorming/${item.id}`;
	}
	if (item.moduleKey === 'kanban') {
		return '/modules/kanban';
	}
	if (item.moduleKey === 'shares') {
		return `/modules/shares/${item.id}`;
	}
	if (item.name?.match(/\.excalidraw$/i)) {
		return `/files?preview=${item.id}`;
	}
	return `/files?preview=${item.id}`;
}

export function cleanArtifactName(name: string): string {
	return name.replace(/\.md$/i, '').replace(/\.jsonl?$/i, '');
}

export function todayDateString(): string {
	return new Date().toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' });
}

export function getUserInitials(name: string | undefined): string {
	if (!name) return '?';
	const parts = name.trim().split(/\s+/);
	if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
	return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

export function getActivityVerb(type: string): string {
	switch (type) {
		case 'file_uploaded':
		case 'folder_created':
			return 'was created';
		case 'file_modified':
			return 'was updated';
		case 'file_downloaded':
			return 'was downloaded';
		case 'file_deleted':
		case 'folder_deleted':
			return 'was deleted';
		case 'file_renamed':
		case 'folder_renamed':
			return 'was renamed';
		case 'file_moved':
		case 'folder_moved':
			return 'was moved';
		case 'share_created':
			return 'was shared';
		case 'share_revoked':
			return 'share was revoked';
		default:
			return 'was updated';
	}
}

export function getModuleColor(moduleKey: string): { color: string; bg: string } {
	const colors: Record<string, { color: string; bg: string }> = {
		notes: { color: '#ea580c', bg: 'rgba(234, 88, 12, 0.1)' },
		meetings: { color: '#7c3aed', bg: 'rgba(124, 58, 237, 0.1)' },
		standups: { color: '#2563eb', bg: 'rgba(37, 99, 235, 0.1)' },
		kanban: { color: '#ea580c', bg: 'rgba(234, 88, 12, 0.1)' },
		decisions: { color: '#16a34a', bg: 'rgba(22, 163, 74, 0.1)' },
		brainstorming: { color: '#ca8a04', bg: 'rgba(202, 138, 4, 0.1)' },
		shares: { color: '#2563eb', bg: 'rgba(37, 99, 235, 0.1)' }
	};
	return colors[moduleKey] ?? { color: '#6b7280', bg: 'rgba(107, 114, 128, 0.1)' };
}

export function getArtifactIcon(moduleKey: string): typeof FileText {
	const map: Record<string, any> = {
		notes: FileText,
		meetings: FileText,
		standups: FileText,
		kanban: Columns,
		decisions: CheckCircle2,
		brainstorming: Lightbulb,
		shares: Share2
	};
	return map[moduleKey] ?? FileText;
}
