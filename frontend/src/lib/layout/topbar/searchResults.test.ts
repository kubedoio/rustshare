import { describe, expect, it } from 'vitest';
import { globalSearchResults } from './searchResults';

describe('globalSearchResults', () => {
	it('keeps attachments and ordinary folders separate from authorized meeting navigation', () => {
		const folderPath = '/Workspace/Meetings/General/2026/Planning';
		const results = [
			{
				source_application: 'io.elembra.files',
				source_type: 'file',
				resource_ref: 'elembra://io.elembra.files/file/attachment',
				title: 'Budget.txt',
				location: `${folderPath}/Budget.txt`,
				snippet: 'The budget is approved',
				provenance: { file_id: 'attachment', meeting_id: 'meeting' }
			},
			{
				source_application: 'io.elembra.files',
				source_type: 'meeting',
				resource_ref: 'elembra://io.elembra.files/file/body',
				title: 'Authorized planning title',
				snippet: 'Historical agenda',
				provenance: { file_id: 'body', meeting_id: 'meeting' }
			}
		];

		const result = globalSearchResults(results);

		expect(result.files.map((file) => file.id)).toEqual(['attachment']);
		expect(result.folders).toEqual([]);
		expect(result.meetings).toEqual([
			{ id: 'meeting', name: 'Authorized planning title', path: '', snippet: 'Historical agenda' }
		]);
	});
});
