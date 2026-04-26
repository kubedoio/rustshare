#!/bin/bash

# PaginationControls.test.ts fix
sed -i 's/function getProps(overrides: Partial<{ currentPage: number; totalPages: number; pageSize: 10 | 20 | 50 }> = {}) {/function getProps(overrides: Partial<{ currentPage: number; totalPages: number; pageSize: 10 | 20 | 50; onPageChange: any; onPageSizeChange: any }> = {}) {/g' frontend/src/lib/components/common/PaginationControls.test.ts
sed -i 's/      onPageChange,/      onPageChange: onPageChange as any,/g' frontend/src/lib/components/common/PaginationControls.test.ts
sed -i 's/      onPageSizeChange,/      onPageSizeChange: onPageSizeChange as any,/g' frontend/src/lib/components/common/PaginationControls.test.ts

# FileListItem.svelte fix
sed -i 's/formatFileSize(fileItem?.size || 0)/formatFileSize(fileItem?.size ?? 0)/g' frontend/src/lib/components/files/FileListItem.svelte
sed -i 's/(typeof (item as Folder).size === '"'"'number'"'"' ? formatFileSize((item as Folder).size) : '"'"'-'"'"')/(typeof (item as Folder).size === '"'"'number'"'"' ? formatFileSize((item as Folder).size as number) : '"'"'-'"'"')/g' frontend/src/lib/components/files/FileListItem.svelte

# FileListRow.svelte fix
sed -i 's/formatFileSize(fileItem?.size || 0)/formatFileSize(fileItem?.size ?? 0)/g' frontend/src/lib/files/FileListRow.svelte
sed -i 's/(typeof (item as Folder).size === '"'"'number'"'"' ? formatFileSize((item as Folder).size) : '"'"'—'"'"')/(typeof (item as Folder).size === '"'"'number'"'"' ? formatFileSize((item as Folder).size as number) : '"'"'—'"'"')/g' frontend/src/lib/files/FileListRow.svelte

# FileGridTile.svelte fix
sed -i 's/formatFileSize(fileItem?.size || 0)/formatFileSize(fileItem?.size ?? 0)/g' frontend/src/lib/files/FileGridTile.svelte
sed -i 's/(typeof (item as Folder).size === '"'"'number'"'"' ? formatFileSize((item as Folder).size) : null)/(typeof (item as Folder).size === '"'"'number'"'"' ? formatFileSize((item as Folder).size as number) : null)/g' frontend/src/lib/files/FileGridTile.svelte

# ShareModal.svelte fix
sed -i 's/enabled: open && activeTab === '"'"'share'"'"'/enabled: open && activeTab === ('"'"'share'"'"' as any)/g' frontend/src/lib/components/modals/ShareModal.svelte

# FileBrowserPane.svelte fix
sed -i 's/<Breadcrumbs {folderPath} {rootLabel} on:navigate={onbreadcrumbNavigate} \/>/<Breadcrumbs {folderPath} {rootLabel} onNavigate={(payload) => onbreadcrumbNavigate(new CustomEvent('"'"'navigate'"'"', { detail: payload }))} \/>/g' frontend/src/lib/files/FileBrowserPane.svelte

# +layout.svelte fix
sed -i 's/<QueryClientProvider client={queryClient}>/<QueryClientProvider client={queryClient} children={null as any}>/g' frontend/src/routes/+layout.svelte

# FileEditorPane.svelte fix
sed -i 's/on:close={onEditorClose}/on:close={() => onEditorClose()}/g' frontend/src/routes/\(app\)/files/FileEditorPane.svelte
sed -i 's/on:saved={onEditorSaved}/on:saved={() => onEditorSaved()}/g' frontend/src/routes/\(app\)/files/FileEditorPane.svelte

# +page.svelte fix
sed -i 's/function handleBreadcrumbNavigate(event: { folderId: string | null }) {/function handleBreadcrumbNavigate(event: CustomEvent<{ folderId: string | null }>) {/g' frontend/src/routes/\(app\)/files/+page.svelte
sed -i 's/const targetId = event.folderId;/const targetId = event.detail.folderId;/g' frontend/src/routes/\(app\)/files/+page.svelte
