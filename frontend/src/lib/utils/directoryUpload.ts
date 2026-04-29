export interface DirectoryUploadItem {
	file: globalThis.File;
	relativePath: string;
}

export function extractFolderPaths(items: DirectoryUploadItem[]): string[] {
	const paths = new Set<string>();
	for (const item of items) {
		const lastSlash = item.relativePath.lastIndexOf('/');
		if (lastSlash > 0) {
			const folderPath = item.relativePath.slice(0, lastSlash);
			const parts = folderPath.split('/');
			let current = '';
			for (const part of parts) {
				current = current ? `${current}/${part}` : part;
				paths.add(current);
			}
		}
	}
	return Array.from(paths);
}

export function sortFolderPaths(paths: string[]): string[] {
	return [...paths]
		.map((path, index) => ({ path, index }))
		.sort((a, b) => {
			const depthA = a.path.split('/').length;
			const depthB = b.path.split('/').length;
			if (depthA !== depthB) {
				return depthA - depthB;
			}
			return a.index - b.index;
		})
		.map(({ path }) => path);
}

export async function collectFilesFromDataTransfer(
	items: DataTransferItemList
): Promise<DirectoryUploadItem[]> {
	const result: DirectoryUploadItem[] = [];

	const readAllEntries = (reader: any): Promise<any[]> => {
		return new Promise((resolve, reject) => {
			const entries: any[] = [];
			const readBatch = () => {
				reader.readEntries((batch: any[]) => {
					if (batch.length === 0) {
						resolve(entries);
					} else {
						entries.push(...batch);
						readBatch();
					}
				}, reject);
			};
			readBatch();
		});
	};

	const readEntry = (entry: any, path: string): Promise<void> => {
		return new Promise((resolve, reject) => {
			if (entry.isFile) {
				entry.file((file: globalThis.File) => {
					const relativePath = path ? `${path}/${file.name}` : file.name;
					(file as any).webkitRelativePath = relativePath;
					result.push({ file, relativePath });
					resolve();
				}, reject);
			} else if (entry.isDirectory) {
				const reader = entry.createReader();
				readAllEntries(reader)
					.then(async (entries) => {
						for (const child of entries) {
							await readEntry(child, path ? `${path}/${entry.name}` : entry.name);
						}
						resolve();
					})
					.catch(reject);
			} else {
				resolve();
			}
		});
	};

	const promises: Promise<void>[] = [];
	for (let i = 0; i < items.length; i++) {
		const entry = (items[i] as any).webkitGetAsEntry?.();
		if (entry) {
			promises.push(readEntry(entry, ''));
		}
	}

	await Promise.all(promises);
	return result;
}
