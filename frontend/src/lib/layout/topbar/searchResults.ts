import type { UnifiedSearchResult } from '$lib/api/search';

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

export function globalSearchResults(results: UnifiedSearchResult[]): SearchResults {
	const meetingMap = new Map<string, SearchItem>();
	const fileMap = new Map<string, SearchItem>();
	for (const result of results) {
		const id = result.provenance.meeting_id;
		if (
			id &&
			result.source_application === 'io.elembra.files' &&
			(result.source_type === 'meeting' || result.source_type === 'meeting_action') &&
			!meetingMap.has(id)
		) {
			meetingMap.set(id, { id, name: result.title, path: '', snippet: result.snippet });
		}
		const fileId = result.provenance.file_id ?? result.provenance.note_id;
		if (fileId && result.source_type === 'file' && !fileMap.has(fileId)) {
			fileMap.set(fileId, {
				id: fileId,
				name: result.title,
				path: result.location ?? '',
				snippet: result.snippet
			});
		}
	}
	return {
		files: [...fileMap.values()],
		folders: [],
		meetings: [...meetingMap.values()]
	};
}
