export const APPROVED_MODULE_ICONS = [
	'layout-dashboard',
	'folder',
	'file-text',
	'sticky-note',
	'calendar-days',
	'clipboard-list',
	'columns',
	'git-branch',
	'share-2',
	'lock',
	'globe',
	'settings'
] as const;

export const DEFAULT_MODULE_ICON = 'folder';

export type ApprovedModuleIcon = (typeof APPROVED_MODULE_ICONS)[number];

export function isApprovedModuleIcon(icon: string): icon is ApprovedModuleIcon {
	return (APPROVED_MODULE_ICONS as readonly string[]).includes(icon);
}
