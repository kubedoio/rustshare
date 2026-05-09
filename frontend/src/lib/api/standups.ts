import { getFile, getFileContent, editFile, renameFile } from './files';

export const standupsApi = {
	get: async (id: string) => {
		const [file, content] = await Promise.all([getFile(id), getFileContent(id)]);
		return {
			...file,
			content,
			metadata: { title: file.name }
		};
	},

	update: async (id: string, req: { title?: string; content?: string }) => {
		if (req.content !== undefined) {
			await editFile(id, req.content, 'overwrite');
		}
		if (req.title !== undefined && req.title !== '') {
			await renameFile(id, req.title);
		}
		const [file, content] = await Promise.all([getFile(id), getFileContent(id)]);
		return {
			...file,
			content,
			metadata: { title: file.name }
		};
	}
};
