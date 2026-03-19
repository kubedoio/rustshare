<script lang="ts">
  import type { File, Folder } from '$lib/api/types';

  export let folders: Folder[] = [];
  export let files: File[] = [];
  export let onFolderClick: (folder: Folder) => void = () => {};
  export let onFileClick: (file: File) => void = () => {};
  export let onRenameFolder: (folder: Folder) => void = () => {};
  export let onDeleteFolder: (folder: Folder) => void = () => {};
  export let onRenameFile: (file: File) => void = () => {};
  export let onDeleteFile: (file: File) => void = () => {};
  export let onShareFile: (file: File) => void = () => {};
  export let onVersionHistory: (file: File) => void = () => {};

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i];
  }

  function formatDate(dateString: string): string {
    const date = new Date(dateString);
    return date.toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
    });
  }

  function getFileIcon(mimeType: string): string {
    if (mimeType.startsWith('image/')) return '🖼️';
    if (mimeType.startsWith('video/')) return '🎥';
    if (mimeType.startsWith('audio/')) return '🎵';
    if (mimeType.includes('pdf')) return '📄';
    if (mimeType.includes('zip') || mimeType.includes('tar')) return '📦';
    if (mimeType.includes('word') || mimeType.includes('document')) return '📝';
    if (mimeType.includes('sheet') || mimeType.includes('excel')) return '📊';
    if (mimeType.includes('presentation')) return '📽️';
    if (mimeType.includes('text/')) return '📃';
    return '📄';
  }

  function handleContextMenu(event: MouseEvent, item: File | Folder, type: 'file' | 'folder') {
    event.preventDefault();
    // Context menu handled by FileListItem
  }
</script>

<div class="overflow-x-auto bg-base-100 rounded-lg shadow">
  <table class="table table-zebra">
    <thead>
      <tr>
        <th>Name</th>
        <th>Type</th>
        <th>Size</th>
        <th>Modified</th>
        <th class="text-right">Actions</th>
      </tr>
    </thead>
    <tbody>
      <!-- Folders -->
      {#each folders as folder}
        <tr class="hover cursor-pointer" on:click={() => onFolderClick(folder)}>
          <td>
            <div class="flex items-center gap-3">
              <span class="text-2xl">📁</span>
              <span class="font-medium">{folder.name}</span>
            </div>
          </td>
          <td>
            <span class="badge badge-ghost">Folder</span>
          </td>
          <td>—</td>
          <td>{formatDate(folder.updated_at)}</td>
          <td class="text-right">
            <div class="dropdown dropdown-end">
              <label tabindex="0" class="btn btn-ghost btn-xs" on:click|stopPropagation>
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z" />
                </svg>
              </label>
              <ul tabindex="0" class="dropdown-content z-[1] menu p-2 shadow bg-base-100 rounded-box w-52">
                <li><button on:click|stopPropagation={() => onRenameFolder(folder)}>Rename</button></li>
                <li><button on:click|stopPropagation={() => onDeleteFolder(folder)} class="text-error">Delete</button></li>
              </ul>
            </div>
          </td>
        </tr>
      {/each}

      <!-- Files -->
      {#each files as file}
        <tr class="hover cursor-pointer" on:click={() => onFileClick(file)}>
          <td>
            <div class="flex items-center gap-3">
              <span class="text-2xl">{getFileIcon(file.mime_type)}</span>
              <span class="font-medium">{file.name}</span>
            </div>
          </td>
          <td>
            <span class="badge badge-ghost text-xs">{file.mime_type.split('/')[0]}</span>
          </td>
          <td>{formatBytes(file.size)}</td>
          <td>{formatDate(file.modified_at)}</td>
          <td class="text-right">
            <div class="dropdown dropdown-end">
              <label tabindex="0" class="btn btn-ghost btn-xs" on:click|stopPropagation>
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-4 h-4">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z" />
                </svg>
              </label>
              <ul tabindex="0" class="dropdown-content z-[1] menu p-2 shadow bg-base-100 rounded-box w-52">
                <li><button on:click|stopPropagation={() => onRenameFile(file)}>Rename</button></li>
                <li><button on:click|stopPropagation={() => onShareFile(file)}>Share</button></li>
                <li><button on:click|stopPropagation={() => onVersionHistory(file)}>Version History</button></li>
                <li><button on:click|stopPropagation={() => onDeleteFile(file)} class="text-error">Delete</button></li>
              </ul>
            </div>
          </td>
        </tr>
      {/each}

      {#if folders.length === 0 && files.length === 0}
        <tr>
          <td colspan="5" class="text-center py-8 text-base-content/50">
            No files or folders
          </td>
        </tr>
      {/if}
    </tbody>
  </table>
</div>
