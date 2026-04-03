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
  return [...paths].sort((a, b) => a.split('/').length - b.split('/').length);
}

export async function collectFilesFromDataTransfer(
  items: DataTransferItemList
): Promise<DirectoryUploadItem[]> {
  const result: DirectoryUploadItem[] = [];

  const readEntry = (entry: any, path: string): Promise<void> => {
    return new Promise((resolve) => {
      if (entry.isFile) {
        entry.file((file: globalThis.File) => {
          const relativePath = path ? `${path}/${file.name}` : file.name;
          (file as any).webkitRelativePath = relativePath;
          result.push({ file, relativePath });
          resolve();
        });
      } else if (entry.isDirectory) {
        const reader = entry.createReader();
        reader.readEntries(async (entries: any[]) => {
          for (const child of entries) {
            await readEntry(child, path ? `${path}/${entry.name}` : entry.name);
          }
          resolve();
        });
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
