import re

def fix_file_grid():
    path = "frontend/src/lib/components/files/FileGrid.svelte"
    with open(path, "r") as f:
        content = f.read()

    # Replace event directives with property callbacks for FileListItem
    content = content.replace("on:rename={(e) => e.detail.isFolder && onRenameFolder(folder)}", "onRename={() => onRenameFolder(folder)}")
    content = content.replace("on:delete={(e) => e.detail.isFolder && onDeleteFolder(folder)}", "onDelete={() => onDeleteFolder(folder)}")
    content = content.replace("on:share={(e) => e.detail.isFolder && onShareFolder(folder)}", "onShare={() => onShareFolder(folder)}")
    content = content.replace("on:move={(e) => e.detail.isFolder && onMoveFolder(folder)}", "onMove={() => onMoveFolder(folder)}")

    content = content.replace("on:rename={(e) => !e.detail.isFolder && onRenameFile(file)}", "onRename={() => onRenameFile(file)}")
    content = content.replace("on:delete={(e) => !e.detail.isFolder && onDeleteFile(file)}", "onDelete={() => onDeleteFile(file)}")
    content = content.replace("on:share={(e) => !e.detail.isFolder && onShareFile(file)}", "onShare={() => onShareFile(file)}")
    content = content.replace("on:versionHistory={handleVersionHistoryClick}", "onVersionHistory={() => onVersionHistory(file)}")
    content = content.replace("on:move={(e) => !e.detail.isFolder && onMoveFile(file)}", "onMove={() => onMoveFile(file)}")
    content = content.replace("on:download={(e) => onDownloadFile(e.detail.item)}", "onDownload={() => onDownloadFile(file)}")
    content = content.replace("on:replace={(e) => onReplaceFile(e.detail.item)}", "onReplace={() => onReplaceFile(file)}")

    with open(path, "w") as f:
        f.write(content)

fix_file_grid()
