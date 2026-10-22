import type { SearchResultItem, UnifiedSearchResult } from '$lib/api/search';
import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';

interface SearchItem {
	id: string;
	name: string;
	path: string;
	snippet?: string;
}

export interface SearchResults {
	files: SearchItem[];
	folders: SearchItem[];
	meetings: SearchItem[];
}

export function globalSearchResults(
	resources: SearchResultItem[],
	historical: UnifiedSearchResult[]
): SearchResults {
	const meetingMap = new Map<string, SearchItem>();
	for (const result of historical) {
		const id = result.provenance.meeting_id;
		if (
			id &&
			result.source_application === 'io.elembra.files' &&
			(result.source_type === 'meeting' || result.source_type === 'meeting_action') &&
			!meetingMap.has(id)
		) {
			meetingMap.set(id, { id, name: result.title, path: '', snippet: result.snippet });
		}
	}
	const visible = filterUserVisibleEntries(resources);
	return {
		files: visible
			.filter((result) => result.resource_type === 'file')
			.map(({ id, name, path }) => ({ id, name, path })),
		folders: visible
			.filter((result) => result.resource_type === 'folder' && !meetingMap.has(result.id))
			.map(({ id, name, path }) => ({ id, name, path })),
		meetings: [...meetingMap.values()]
	};
}
