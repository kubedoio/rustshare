# File Upload Quick Reference

## For Developers

### Adding Upload Functionality to a New Page

```typescript
// 1. Import required components and types
import UploadButton from '$lib/components/files/UploadButton.svelte';
import UploadProgress from '$lib/components/files/UploadProgress.svelte';
import DropZone from '$lib/components/files/DropZone.svelte';
import { uploadFile } from '$lib/api/files';
import type { UploadTask } from '$lib/components/files/UploadProgress.svelte';

// 2. Create state for upload tasks
let uploadTasks: UploadTask[] = [];

// 3. Create upload handler
async function handleFilesSelected(files: globalThis.File[]) {
  // Generate previews for images
  const newTasks = await Promise.all(
    files.map(async (file) => ({
      id: `${file.name}-${Date.now()}-${Math.random()}`,
      fileName: file.name,
      size: file.size,
      status: 'pending' as const,
      progress: 0,
      previewUrl: await generatePreview(file)
    }))
  );

  uploadTasks = [...uploadTasks, ...newTasks];

  // Upload files
  for (let i = 0; i < files.length; i++) {
    const taskIndex = uploadTasks.findIndex((t) => t.id === newTasks[i].id);

    uploadTasks[taskIndex] = { ...uploadTasks[taskIndex], status: 'uploading' };
    uploadTasks = [...uploadTasks];

    try {
      await uploadFile(currentFolderId, files[i]);
      uploadTasks[taskIndex] = { ...uploadTasks[taskIndex], status: 'success' };
    } catch (error) {
      uploadTasks[taskIndex] = {
        ...uploadTasks[taskIndex],
        status: 'error',
        error: error.message
      };
    }
    uploadTasks = [...uploadTasks];
  }
}

// 4. Add preview generation helper
async function generatePreview(file: globalThis.File): Promise<string | undefined> {
  if (!file.type.startsWith('image/')) return undefined;

  return new Promise((resolve) => {
    const reader = new FileReader();
    reader.onload = (e) => {
      const img = new Image();
      img.onload = () => {
        const canvas = document.createElement('canvas');
        const ctx = canvas.getContext('2d');
        if (!ctx) { resolve(undefined); return; }

        const maxSize = 96;
        let width = img.width;
        let height = img.height;

        if (width > height) {
          if (width > maxSize) {
            height = (height * maxSize) / width;
            width = maxSize;
          }
        } else {
          if (height > maxSize) {
            width = (width * maxSize) / height;
            height = maxSize;
          }
        }

        canvas.width = width;
        canvas.height = height;
        ctx.drawImage(img, 0, 0, width, height);
        resolve(canvas.toDataURL('image/jpeg', 0.8));
      };
      img.onerror = () => resolve(undefined);
      img.src = e.target?.result as string;
    };
    reader.onerror = () => resolve(undefined);
    reader.readAsDataURL(file);
  });
}
```

```svelte
<!-- 5. Add to template -->
<DropZone on:filesDropped={(e) => handleFilesSelected(e.detail)}>
  <div>
    <!-- Your page content -->

    <UploadButton
      on:filesSelected={(e) => handleFilesSelected(e.detail)}
      disabled={isUploading}
    />
  </div>
</DropZone>

<UploadProgress
  tasks={uploadTasks}
  onClose={() => uploadTasks = []}
/>
```

## Component Props Reference

### UploadButton
```typescript
{
  disabled?: boolean;    // Disable button
  multiple?: boolean;    // Allow multiple files (default: true)
}

// Events
on:filesSelected={(e) => {
  const files: globalThis.File[] = e.detail;
}}
```

### UploadProgress
```typescript
{
  tasks: UploadTask[];   // Array of upload tasks
  onClose: () => void;   // Close handler
}

interface UploadTask {
  id: string;              // Unique ID
  fileName: string;        // Display name
  size: number;           // Bytes
  status: 'pending' | 'uploading' | 'success' | 'error';
  progress: number;       // 0-100
  error?: string;         // Error message
  previewUrl?: string;    // Data URL for thumbnail
}
```

### DropZone
```typescript
{
  disabled?: boolean;    // Disable drop zone
}

// Events
on:filesDropped={(e) => {
  const files: globalThis.File[] = e.detail;
}}
```

## API Reference

### uploadFile()
```typescript
import { uploadFile } from '$lib/api/files';

// Upload a file
const result = await uploadFile(
  folderId,  // string | null - parent folder ID or null for root
  file       // globalThis.File - the file to upload
);

// Returns: File object
{
  id: string;
  name: string;
  size: number;
  mime_type: string;
  content_hash: string;
  current_version: number;
  created_at: string;
  modified_at: string;
  path: string;
  parent_folder_id: string | null;
  owner_id: string;
}
```

## Common Patterns

### Simple Upload (No Preview)
```typescript
async function handleUpload(files: globalThis.File[]) {
  for (const file of files) {
    try {
      await uploadFile(null, file);
      console.log('Uploaded:', file.name);
    } catch (error) {
      console.error('Failed:', file.name, error);
    }
  }
}
```

### Upload with Progress Tracking
```typescript
let uploadTasks: UploadTask[] = [];

async function handleUpload(files: globalThis.File[]) {
  // Create tasks
  const newTasks = files.map(file => ({
    id: `${file.name}-${Date.now()}`,
    fileName: file.name,
    size: file.size,
    status: 'pending' as const,
    progress: 0
  }));

  uploadTasks = [...uploadTasks, ...newTasks];

  // Upload each
  for (let i = 0; i < files.length; i++) {
    const taskIndex = uploadTasks.findIndex(t => t.id === newTasks[i].id);
    uploadTasks[taskIndex].status = 'uploading';
    uploadTasks = [...uploadTasks];

    try {
      await uploadFile(null, files[i]);
      uploadTasks[taskIndex].status = 'success';
    } catch (error) {
      uploadTasks[taskIndex].status = 'error';
      uploadTasks[taskIndex].error = error.message;
    }
    uploadTasks = [...uploadTasks];
  }
}
```

### Upload to Specific Folder
```typescript
const targetFolderId = 'folder-uuid-123';

async function handleUpload(files: globalThis.File[]) {
  for (const file of files) {
    await uploadFile(targetFolderId, file);
  }
}
```

### Upload with Query Invalidation (TanStack Query)
```typescript
import { createMutation } from '@tanstack/svelte-query';
import { queryClient } from '$lib/query-client';

const uploadMutation = createMutation({
  mutationFn: async (file: globalThis.File) => {
    return uploadFile(currentFolderId, file);
  },
  onSuccess: () => {
    // Refresh file list
    queryClient.invalidateQueries({
      queryKey: ['folder-contents', currentFolderId]
    });
  }
});

async function handleUpload(files: globalThis.File[]) {
  for (const file of files) {
    await $uploadMutation.mutateAsync(file);
  }
}
```

### Upload with Activity Logging
```typescript
import { activityStore } from '$lib/stores/activity';

async function handleUpload(files: globalThis.File[]) {
  for (const file of files) {
    try {
      await uploadFile(null, file);
      activityStore.addActivity('file_uploaded', file.name);
    } catch (error) {
      console.error('Upload failed:', error);
    }
  }
}
```

## Styling

### Custom Button Style
```svelte
<UploadButton>
  <!-- Default styling uses DaisyUI: btn btn-primary -->

  <!-- To customize, modify UploadButton.svelte or wrap in custom button -->
</UploadButton>
```

### Custom Progress Panel Position
```svelte
<UploadProgress tasks={uploadTasks} onClose={handleClose} />

<!-- Default: fixed bottom-4 right-4 -->

<!-- To customize, modify UploadProgress.svelte class:
     "fixed bottom-4 right-4 w-96 bg-base-100 shadow-xl rounded-lg"
-->
```

### Custom Drop Zone Style
```svelte
<DropZone>
  <!-- Drop zone wraps content invisibly -->
  <!-- Overlay appears on drag with:
       - bg-primary/10
       - border-4 border-dashed border-primary
  -->
</DropZone>
```

## Error Handling

### Handling Upload Errors
```typescript
async function handleUpload(files: globalThis.File[]) {
  const errors: Array<{ file: string; error: string }> = [];

  for (const file of files) {
    try {
      await uploadFile(null, file);
    } catch (error) {
      if (error instanceof ApiError) {
        errors.push({
          file: file.name,
          error: error.message
        });
      } else {
        errors.push({
          file: file.name,
          error: 'Unknown error'
        });
      }
    }
  }

  if (errors.length > 0) {
    console.error('Upload errors:', errors);
    showErrorNotification(`${errors.length} file(s) failed to upload`);
  }
}
```

### Handling Network Errors
```typescript
import { ApiError } from '$lib/api/types';

try {
  await uploadFile(null, file);
} catch (error) {
  if (error instanceof ApiError) {
    if (error.status === 401) {
      // Unauthorized - redirect to login
      goto('/login');
    } else if (error.status === 413) {
      // File too large
      showErrorNotification('File is too large');
    } else if (error.status >= 500) {
      // Server error
      showErrorNotification('Server error, please try again');
    } else {
      // Other API error
      showErrorNotification(error.message);
    }
  } else {
    // Network error or other issue
    showErrorNotification('Upload failed. Please check your connection.');
  }
}
```

## Testing

### Manual Testing
```typescript
// Test with various file types
const testFiles = [
  new File(['test'], 'test.txt', { type: 'text/plain' }),
  new File([new Uint8Array(1024)], 'test.jpg', { type: 'image/jpeg' }),
  new File([new Uint8Array(1024 * 1024)], 'large.pdf', { type: 'application/pdf' })
];

handleFilesSelected(testFiles);
```

### Unit Testing
```typescript
import { describe, it, expect, vi } from 'vitest';
import { uploadFile } from '$lib/api/files';

describe('uploadFile', () => {
  it('should upload file successfully', async () => {
    const file = new File(['test'], 'test.txt', { type: 'text/plain' });
    const result = await uploadFile(null, file);
    expect(result.name).toBe('test.txt');
  });

  it('should handle upload error', async () => {
    const file = new File(['test'], 'test.txt', { type: 'text/plain' });
    await expect(uploadFile('invalid-id', file)).rejects.toThrow();
  });
});
```

## Performance Tips

1. **Generate previews in parallel**: Use `Promise.all()` for multiple images
2. **Upload sequentially**: Avoid overwhelming the server
3. **Limit file size**: Validate before upload
4. **Use compression**: Compress large images before upload
5. **Lazy load thumbnails**: Only generate when needed

## Troubleshooting

### Upload button not working
- Check if `disabled` prop is set
- Verify event handler is connected
- Check browser console for errors

### Preview not showing
- Verify file is an image (MIME type)
- Check browser support for FileReader
- Look for canvas rendering errors

### Upload fails silently
- Check network tab in dev tools
- Verify API endpoint is correct
- Check authentication token
- Look for CORS errors

### Progress panel not appearing
- Verify `uploadTasks` has items
- Check if panel is hidden by z-index
- Ensure component is mounted

## Browser Compatibility

| Feature | Chrome | Firefox | Safari | Edge |
|---------|--------|---------|--------|------|
| File API | ✅ | ✅ | ✅ | ✅ |
| Drag/Drop | ✅ | ✅ | ✅ | ✅ |
| FileReader | ✅ | ✅ | ✅ | ✅ |
| Canvas | ✅ | ✅ | ✅ | ✅ |
| FormData | ✅ | ✅ | ✅ | ✅ |

All features work in modern browsers (last 2 versions).
