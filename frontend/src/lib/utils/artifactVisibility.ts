export const INTERNAL_RUSTSHARE_FILES = [
	'.rustshare.json',
	'.rustshare-board.json',
	'.rustshare-column.json',
	'.rustshare-card.json',
	'.rustshare-share.json',
	'events.jsonl',
	'index.md',
	'__primary__.md',
	'index.editor.json'
];

export function isInternalRustShareFile(name: string): boolean {
	if (!name) return false;
	const lower = name.toLowerCase();
	return (
		INTERNAL_RUSTSHARE_FILES.includes(lower) ||
		lower.endsWith('.rustshare.json') ||
		lower.endsWith('.editor.json') ||
		lower.startsWith('.rustshare-')
	);
}

export function filterUserVisibleEntries<T extends { name: string }>(entries: T[]): T[] {
	return entries.filter((e) => !isInternalRustShareFile(e.name));
}
