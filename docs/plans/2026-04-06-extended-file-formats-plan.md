# Extended File Format Support Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add preview and edit support for MS Office formats (metadata), image editing (rotate, flip, resize, crop), and config file editing (JSON/YAML/CONF) — all client-side.

**Architecture:** Extend existing file type detection and preview/edit components. Add new ImageEditor component using Canvas API, extend Monaco editor language support, and create Office file preview component.

**Tech Stack:** Svelte, TypeScript, Monaco Editor, HTML5 Canvas API, Tailwind CSS

---

## Prerequisites

- Existing codebase structure understood (see design doc: `docs/plans/2026-04-06-extended-file-formats-design.md`)
- Frontend uses Svelte with existing FilePreviewModal component
- Monaco editor already integrated for text editing

---

## Task 1: Update File Type Utilities

**Files:**
- Modify: `frontend/src/lib/utils/editor.ts`
- Modify: `frontend/src/lib/utils/format.ts`

**Step 1: Add Office MIME type detection to format.ts**

Add after existing `getFileTypeLabel` function:

```typescript
/**
 * Check if a file is an MS Office document
 */
export function isOfficeFile(mimeType: string, fileName: string): boolean {
  const normalized = mimeType.toLowerCase();
  const name = fileName.toLowerCase();
  
  const officeMimeTypes = [
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    'application/msword',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    'application/vnd.ms-excel',
    'application/vnd.openxmlformats-officedocument.presentationml.presentation',
    'application/vnd.ms-powerpoint'
  ];
  
  const officeExtensions = ['.docx', '.doc', '.xlsx', '.xls', '.pptx', '.ppt'];
  
  if (officeMimeTypes.some(m => normalized.includes(m))) return true;
  if (officeExtensions.some(ext => name.endsWith(ext))) return true;
  
  return false;
}

/**
 * Get Office file type label
 */
export function getOfficeFileType(mimeType: string, fileName: string): 'word' | 'excel' | 'powerpoint' | null {
  const normalized = mimeType.toLowerCase();
  const name = fileName.toLowerCase();
  
  if (normalized.includes('wordprocessingml') || normalized.includes('msword') || name.endsWith('.docx') || name.endsWith('.doc')) {
    return 'word';
  }
  if (normalized.includes('spreadsheetml') || normalized.includes('ms-excel') || name.endsWith('.xlsx') || name.endsWith('.xls')) {
    return 'excel';
  }
  if (normalized.includes('presentationml') || normalized.includes('ms-powerpoint') || name.endsWith('.pptx') || name.endsWith('.ppt')) {
    return 'powerpoint';
  }
  
  return null;
}
```

**Step 2: Update editor.ts to add image and config file detection**

Replace the `detectEditorType` function with more comprehensive detection:

```typescript
export type EditorType = 'text' | 'markdown' | 'excalidraw' | 'image' | 'none';
export type PreviewType = 'image' | 'pdf' | 'video' | 'audio' | 'text' | 'office' | 'code' | 'none';

export interface FileCapabilities {
  editorType: EditorType;
  previewType: PreviewType;
  language?: string;
  canEdit: boolean;
}

/**
 * Detects file capabilities based on name and MIME type
 */
export function detectFileCapabilities(fileName: string, mimeType: string): FileCapabilities {
  const name = fileName.toLowerCase();
  const mime = mimeType.toLowerCase();
  
  // Images
  if (mime.startsWith('image/')) {
    return {
      editorType: 'image',
      previewType: 'image',
      canEdit: true
    };
  }
  
  // PDF
  if (mime === 'application/pdf') {
    return {
      editorType: 'none',
      previewType: 'pdf',
      canEdit: false
    };
  }
  
  // Video
  if (mime.startsWith('video/')) {
    return {
      editorType: 'none',
      previewType: 'video',
      canEdit: false
    };
  }
  
  // Audio
  if (mime.startsWith('audio/')) {
    return {
      editorType: 'none',
      previewType: 'audio',
      canEdit: false
    };
  }
  
  // Office files
  if (isOfficeFile(mimeType, fileName)) {
    return {
      editorType: 'none',
      previewType: 'office',
      canEdit: false
    };
  }
  
  // Excalidraw
  if (name.endsWith('.excalidraw') || name.endsWith('.excalidraw.json')) {
    return {
      editorType: 'excalidraw',
      previewType: 'text',
      canEdit: true
    };
  }
  
  // Markdown
  if (name.endsWith('.md') || name.endsWith('.mdx') || mime === 'text/markdown') {
    return {
      editorType: 'markdown',
      previewType: 'text',
      language: 'markdown',
      canEdit: true
    };
  }
  
  // Config files with specific languages
  const ext = name.split('.').pop() || '';
  const configLanguages: Record<string, string> = {
    'json': 'json',
    'yaml': 'yaml',
    'yml': 'yaml',
    'toml': 'toml',
    'ini': 'ini',
    'conf': 'ini',
    'config': 'ini',
    'env': 'plaintext'
  };
  
  if (configLanguages[ext]) {
    return {
      editorType: 'text',
      previewType: 'code',
      language: configLanguages[ext],
      canEdit: true
    };
  }
  
  // Text files
  if (mime.startsWith('text/') || isCodeFile(name)) {
    return {
      editorType: 'text',
      previewType: 'code',
      language: getMonacoLanguage(fileName),
      canEdit: true
    };
  }
  
  return {
    editorType: 'none',
    previewType: 'none',
    canEdit: false
  };
}

// Helper function (add if not exists)
function isOfficeFile(mimeType: string, fileName: string): boolean {
  const normalized = mimeType.toLowerCase();
  const name = fileName.toLowerCase();
  
  const officeMimeTypes = [
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    'application/msword',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    'application/vnd.ms-excel',
    'application/vnd.openxmlformats-officedocument.presentationml.presentation',
    'application/vnd.ms-powerpoint'
  ];
  
  const officeExtensions = ['.docx', '.doc', '.xlsx', '.xls', '.pptx', '.ppt'];
  
  return officeMimeTypes.some(m => normalized.includes(m)) || 
         officeExtensions.some(ext => name.endsWith(ext));
}
```

**Step 3: Run tests to verify no regressions**

```bash
cd frontend
npm test -- src/lib/utils/format.test.ts
npm test -- src/lib/utils/editor.test.ts  # if exists
```

**Step 4: Commit**

```bash
git add frontend/src/lib/utils/editor.ts frontend/src/lib/utils/format.ts
git commit -m "feat: extend file type detection for office, images, and config files"
```

---

## Task 2: Create Office Preview Component

**Files:**
- Create: `frontend/src/lib/components/preview/OfficePreview.svelte`

**Step 1: Create the OfficePreview component**

```svelte
<script lang="ts">
  import type { File } from '$lib/api/types';
  import { formatFileSize } from '$lib/utils/format';
  import { getOfficeFileType } from '$lib/utils/format';
  import { FileText, FileSpreadsheet, FilePresentation } from 'lucide-svelte';
  
  export let file: File;
  
  $: officeType = getOfficeFileType(file.mime_type, file.name);
  
  const officeConfig = {
    word: {
      icon: FileText,
      label: 'Microsoft Word Document',
      color: 'text-blue-500',
      bgColor: 'bg-blue-50'
    },
    excel: {
      icon: FileSpreadsheet,
      label: 'Microsoft Excel Spreadsheet',
      color: 'text-green-500',
      bgColor: 'bg-green-50'
    },
    powerpoint: {
      icon: FilePresentation,
      label: 'Microsoft PowerPoint Presentation',
      color: 'text-orange-500',
      bgColor: 'bg-orange-50'
    }
  };
  
  $: config = officeType ? officeConfig[officeType] : null;
</script>

<div class="flex flex-col items-center justify-center p-12 text-center">
  {#if config}
    <div class="w-24 h-24 rounded-2xl {config.bgColor} flex items-center justify-center mb-6">
      <svelte:component this={config.icon} size={48} class={config.color} />
    </div>
    <h3 class="text-xl font-semibold text-base-content mb-2">{file.name}</h3>
    <p class="text-base-content/60 mb-1">{config.label}</p>
    <p class="text-sm text-base-content/40 mb-6">
      {formatFileSize(file.size)} • {new Date(file.modified_at).toLocaleDateString()}
    </p>
    <div class="flex flex-col items-center gap-4">
      <p class="text-sm text-base-content/60 max-w-sm">
        This file type cannot be previewed in the browser. 
        Please download the file to view it.
      </p>
      <slot name="download-button" />
    </div>
  {:else}
    <div class="w-20 h-20 rounded-2xl bg-base-300 flex items-center justify-center mb-4">
      <svg xmlns="http://www.w3.org/2000/svg" class="w-10 h-10 text-base-content/40" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
      </svg>
    </div>
    <h3 class="text-lg font-semibold text-base-content mb-2">{file.name}</h3>
    <p class="text-sm text-base-content/60">Office Document</p>
  {/if}
</div>
```

**Step 2: Export from index if needed**

Add to `frontend/src/lib/components/index.ts` if it exists, or ensure component can be imported directly.

**Step 3: Commit**

```bash
git add frontend/src/lib/components/preview/OfficePreview.svelte
git commit -m "feat: add OfficePreview component for docx/xlsx/pptx files"
```

---

## Task 3: Update FilePreviewModal for Office and Code Files

**Files:**
- Modify: `frontend/src/lib/components/modals/FilePreviewModal.svelte`

**Step 1: Add imports and update detection logic**

Replace the import section and type detection:

```typescript
import OfficePreview from '$lib/components/preview/OfficePreview.svelte';
import { detectFileCapabilities } from '$lib/utils/editor';

// Replace the existing detection logic (~line 32-33)
$: capabilities = file ? detectFileCapabilities(file.name, file.mime_type) : null;
$: canEdit = capabilities?.canEdit ?? false;
```

**Step 2: Update the preview rendering section**

Replace the preview content section (around line 202-262) to use capabilities:

```svelte
{:else if file}
  {#if capabilities?.previewType === 'image' && previewUrl}
    <img src={previewUrl} alt={file.name} class="max-h-full max-w-full object-contain" />
  
  {:else if capabilities?.previewType === 'pdf' && previewUrl}
    <iframe src={previewUrl} title={file.name} class="h-full w-full" frameborder="0"></iframe>
  
  {:else if capabilities?.previewType === 'video' && previewUrl}
    <video src={previewUrl} controls class="max-h-full max-w-full">
      <track kind="captions" />
      Your browser doesn't support video playback.
    </video>
  
  {:else if capabilities?.previewType === 'audio' && previewUrl}
    <div class="p-8">
      <audio src={previewUrl} controls class="w-full">
        Your browser doesn't support audio playback.
      </audio>
    </div>
  
  {:else if capabilities?.previewType === 'office'}
    <OfficePreview {file}>
      <button slot="download-button" class="btn btn-primary" on:click={handleDownload}>
        <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 mr-2" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
        </svg>
        Download to View
      </button>
    </OfficePreview>
  
  {:else if capabilities?.previewType === 'code' && textContent !== null}
    <div class="w-full h-full overflow-auto bg-base-100">
      <pre class="p-6 font-mono text-sm"><code>{textContent}</code></pre>
    </div>
  
  {:else if file.name.toLowerCase().endsWith('.md') && textContent !== null}
    <div class="w-full h-full p-8 overflow-auto bg-base-100">
      <article class="prose max-w-none">
        {@html renderMarkdown(textContent)}
      </article>
    </div>
  
  {:else if (isExcalidraw(file.name) || isDrawio(file.name)) && textContent !== null}
    <div class="p-12 text-center flex flex-col items-center gap-4">
      <div class="w-20 h-20 bg-primary/10 rounded-2xl flex items-center justify-center text-primary mb-2">
        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-10 h-10">
          <path stroke-linecap="round" stroke-linejoin="round" d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125M18 14v4.75A2.25 2.25 0 0115.75 21H5.25A2.25 2.25 0 013 18.75V8.25A2.25 2.25 0 015.25 6H10" />
        </svg>
      </div>
      <h4 class="text-xl font-bold">{isExcalidraw(file.name) ? 'Excalidraw' : 'Draw.io'} Diagram</h4>
      <p class="text-base-content/60 max-w-md">
        This file is a diagram that can be viewed and edited in the specialized editor.
      </p>
      <button class="btn btn-primary mt-4" on:click={handleEdit}>
        Open in Editor
      </button>
    </div>
  
  {:else}
    <div class="p-8 text-center">
      <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-16 h-16 text-base-content/40 mb-4 mx-auto">
        <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
      </svg>
      <p class="text-base-content/60 mb-4">Preview not available for this file type</p>
      <button class="btn btn-primary" on:click={handleDownload}>Download File</button>
    </div>
  {/if}
{/if}
```

**Step 3: Update the canPreview function**

Update to use capabilities:

```typescript
function canPreview(file: File): boolean {
  const caps = detectFileCapabilities(file.name, file.mime_type);
  return caps.previewType !== 'none';
}
```

**Step 4: Test the modal**

```bash
cd frontend
npm run build
```

Check for TypeScript errors.

**Step 5: Commit**

```bash
git add frontend/src/lib/components/modals/FilePreviewModal.svelte
git commit -m "feat: update FilePreviewModal to support office and config file previews"
```

---

## Task 4: Create Image Editor Core Logic

**Files:**
- Create: `frontend/src/lib/utils/imageEditor.ts`

**Step 1: Create the ImageEditor class**

```typescript
export interface CropSelection {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ImageDimensions {
  width: number;
  height: number;
}

export class ImageEditor {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private history: ImageData[] = [];
  private historyIndex = -1;
  private maxHistorySize = 20;
  private originalImage: HTMLImageElement | null = null;
  
  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    const ctx = canvas.getContext('2d');
    if (!ctx) {
      throw new Error('Could not get 2D context from canvas');
    }
    this.ctx = ctx;
  }
  
  async loadImage(src: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const img = new Image();
      img.crossOrigin = 'anonymous';
      img.onload = () => {
        this.originalImage = img;
        this.canvas.width = img.width;
        this.canvas.height = img.height;
        this.ctx.drawImage(img, 0, 0);
        this.saveState();
        resolve();
      };
      img.onerror = reject;
      img.src = src;
    });
  }
  
  loadFromFile(file: File): Promise<void> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = (e) => {
        const result = e.target?.result as string;
        if (result) {
          this.loadImage(result).then(resolve).catch(reject);
        } else {
          reject(new Error('Failed to read file'));
        }
      };
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
  }
  
  private saveState(): void {
    // Remove any states after current index (for redo support)
    if (this.historyIndex < this.history.length - 1) {
      this.history = this.history.slice(0, this.historyIndex + 1);
    }
    
    // Add new state
    this.history.push(this.ctx.getImageData(0, 0, this.canvas.width, this.canvas.height));
    
    // Limit history size
    if (this.history.length > this.maxHistorySize) {
      this.history.shift();
    } else {
      this.historyIndex++;
    }
  }
  
  undo(): boolean {
    if (this.historyIndex > 0) {
      this.historyIndex--;
      this.ctx.putImageData(this.history[this.historyIndex], 0, 0);
      return true;
    }
    return false;
  }
  
  redo(): boolean {
    if (this.historyIndex < this.history.length - 1) {
      this.historyIndex++;
      this.ctx.putImageData(this.history[this.historyIndex], 0, 0);
      return true;
    }
    return false;
  }
  
  canUndo(): boolean {
    return this.historyIndex > 0;
  }
  
  canRedo(): boolean {
    return this.historyIndex < this.history.length - 1;
  }
  
  rotateClockwise(): void {
    this.rotate(90);
  }
  
  rotateCounterClockwise(): void {
    this.rotate(-90);
  }
  
  private rotate(degrees: 90 | -90): void {
    const tempCanvas = document.createElement('canvas');
    const tempCtx = tempCanvas.getContext('2d')!;
    
    tempCanvas.width = this.canvas.height;
    tempCanvas.height = this.canvas.width;
    
    tempCtx.translate(tempCanvas.width / 2, tempCanvas.height / 2);
    tempCtx.rotate((degrees * Math.PI) / 180);
    tempCtx.drawImage(this.canvas, -this.canvas.width / 2, -this.canvas.height / 2);
    
    this.canvas.width = tempCanvas.width;
    this.canvas.height = tempCanvas.height;
    this.ctx.drawImage(tempCanvas, 0, 0);
    
    this.saveState();
  }
  
  flipHorizontal(): void {
    this.flip('horizontal');
  }
  
  flipVertical(): void {
    this.flip('vertical');
  }
  
  private flip(direction: 'horizontal' | 'vertical'): void {
    const tempCanvas = document.createElement('canvas');
    const tempCtx = tempCanvas.getContext('2d')!;
    
    tempCanvas.width = this.canvas.width;
    tempCanvas.height = this.canvas.height;
    
    tempCtx.translate(
      direction === 'horizontal' ? tempCanvas.width : 0,
      direction === 'vertical' ? tempCanvas.height : 0
    );
    tempCtx.scale(
      direction === 'horizontal' ? -1 : 1,
      direction === 'vertical' ? -1 : 1
    );
    tempCtx.drawImage(this.canvas, 0, 0);
    
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    this.ctx.drawImage(tempCanvas, 0, 0);
    
    this.saveState();
  }
  
  resize(width: number, height: number): void {
    const tempCanvas = document.createElement('canvas');
    const tempCtx = tempCanvas.getContext('2d')!;
    
    tempCanvas.width = width;
    tempCanvas.height = height;
    
    // Use better quality downsampling
    tempCtx.imageSmoothingEnabled = true;
    tempCtx.imageSmoothingQuality = 'high';
    
    tempCtx.drawImage(this.canvas, 0, 0, width, height);
    
    this.canvas.width = width;
    this.canvas.height = height;
    this.ctx.drawImage(tempCanvas, 0, 0);
    
    this.saveState();
  }
  
  crop(selection: CropSelection): void {
    const tempCanvas = document.createElement('canvas');
    const tempCtx = tempCanvas.getContext('2d')!;
    
    tempCanvas.width = selection.width;
    tempCanvas.height = selection.height;
    
    tempCtx.drawImage(
      this.canvas,
      selection.x, selection.y, selection.width, selection.height,
      0, 0, selection.width, selection.height
    );
    
    this.canvas.width = selection.width;
    this.canvas.height = selection.height;
    this.ctx.drawImage(tempCanvas, 0, 0);
    
    this.saveState();
  }
  
  getDimensions(): ImageDimensions {
    return {
      width: this.canvas.width,
      height: this.canvas.height
    };
  }
  
  toBlob(type = 'image/png', quality = 0.92): Promise<Blob> {
    return new Promise((resolve, reject) => {
      this.canvas.toBlob(
        (blob) => {
          if (blob) {
            resolve(blob);
          } else {
            reject(new Error('Failed to create blob from canvas'));
          }
        },
        type,
        quality
      );
    });
  }
  
  toFile(filename: string, type = 'image/png'): Promise<File> {
    return this.toBlob(type).then(blob => 
      new File([blob], filename, { type })
    );
  }
  
  reset(): void {
    if (this.originalImage) {
      this.canvas.width = this.originalImage.width;
      this.canvas.height = this.originalImage.height;
      this.ctx.drawImage(this.originalImage, 0, 0);
      this.history = [];
      this.historyIndex = -1;
      this.saveState();
    }
  }
}
```

**Step 2: Create unit tests**

Create: `frontend/src/lib/utils/imageEditor.test.ts`

```typescript
import { describe, it, expect, beforeEach } from 'vitest';
import { ImageEditor } from './imageEditor';

describe('ImageEditor', () => {
  let canvas: HTMLCanvasElement;
  let editor: ImageEditor;
  
  beforeEach(() => {
    canvas = document.createElement('canvas');
    canvas.width = 100;
    canvas.height = 100;
    editor = new ImageEditor(canvas);
  });
  
  it('should initialize with canvas', () => {
    expect(editor).toBeDefined();
    expect(editor.getDimensions()).toEqual({ width: 100, height: 100 });
  });
  
  it('should track undo/redo state', () => {
    expect(editor.canUndo()).toBe(false);
    expect(editor.canRedo()).toBe(false);
  });
  
  it('should flip dimensions on rotate', async () => {
    // Create a simple test by drawing something
    const ctx = canvas.getContext('2d')!;
    ctx.fillStyle = 'red';
    ctx.fillRect(0, 0, 100, 100);
    
    editor.rotateClockwise();
    
    const dims = editor.getDimensions();
    expect(dims.width).toBe(100);
    expect(dims.height).toBe(100);
  });
});
```

**Step 3: Run tests**

```bash
cd frontend
npm test -- src/lib/utils/imageEditor.test.ts
```

**Step 4: Commit**

```bash
git add frontend/src/lib/utils/imageEditor.ts frontend/src/lib/utils/imageEditor.test.ts
git commit -m "feat: add ImageEditor utility class with rotate, flip, resize, crop operations"
```

---

## Task 5: Create Image Editor Svelte Component

**Files:**
- Create: `frontend/src/lib/components/editors/ImageEditor.svelte`

**Step 1: Create the ImageEditor component**

```svelte
<script lang="ts">
  import { onMount, onDestroy, createEventDispatcher } from 'svelte';
  import { ImageEditor } from '$lib/utils/imageEditor';
  import type { CropSelection } from '$lib/utils/imageEditor';
  import { RotateCw, RotateCcw, FlipHorizontal, FlipVertical, Scissors, Undo, Redo, X, Check } from 'lucide-svelte';
  
  export let imageUrl: string;
  export let fileName: string;
  
  const dispatch = createEventDispatcher<{
    save: { blob: Blob; fileName: string };
    cancel: void;
  }>();
  
  let canvas: HTMLCanvasElement;
  let editor: ImageEditor;
  let loading = true;
  let error: string | null = null;
  
  // Toolbar state
  let canUndo = false;
  let canRedo = false;
  
  // Crop mode
  let isCropping = false;
  let cropStart: { x: number; y: number } | null = null;
  let cropSelection: CropSelection | null = null;
  let isDragging = false;
  
  // Resize modal
  let showResizeModal = false;
  let resizeWidth = 0;
  let resizeHeight = 0;
  let maintainAspectRatio = true;
  let aspectRatio = 1;
  
  onMount(async () => {
    try {
      editor = new ImageEditor(canvas);
      await editor.loadImage(imageUrl);
      updateToolbarState();
      const dims = editor.getDimensions();
      resizeWidth = dims.width;
      resizeHeight = dims.height;
      aspectRatio = dims.width / dims.height;
      loading = false;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load image';
      loading = false;
    }
  });
  
  function updateToolbarState() {
    canUndo = editor?.canUndo() ?? false;
    canRedo = editor?.canRedo() ?? false;
  }
  
  function handleRotateCw() {
    editor.rotateClockwise();
    updateToolbarState();
  }
  
  function handleRotateCcw() {
    editor.rotateCounterClockwise();
    updateToolbarState();
  }
  
  function handleFlipH() {
    editor.flipHorizontal();
    updateToolbarState();
  }
  
  function handleFlipV() {
    editor.flipVertical();
    updateToolbarState();
  }
  
  function handleUndo() {
    editor.undo();
    updateToolbarState();
  }
  
  function handleRedo() {
    editor.redo();
    updateToolbarState();
  }
  
  // Crop handlers
  function startCropMode() {
    isCropping = true;
    cropSelection = null;
  }
  
  function cancelCrop() {
    isCropping = false;
    cropSelection = null;
    cropStart = null;
  }
  
  function applyCrop() {
    if (cropSelection && editor) {
      editor.crop(cropSelection);
      updateToolbarState();
    }
    isCropping = false;
    cropSelection = null;
    cropStart = null;
  }
  
  function handleCanvasMouseDown(e: MouseEvent) {
    if (!isCropping) return;
    
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    
    cropStart = {
      x: (e.clientX - rect.left) * scaleX,
      y: (e.clientY - rect.top) * scaleY
    };
    isDragging = true;
  }
  
  function handleCanvasMouseMove(e: MouseEvent) {
    if (!isCropping || !isDragging || !cropStart) return;
    
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    
    const currentX = (e.clientX - rect.left) * scaleX;
    const currentY = (e.clientY - rect.top) * scaleY;
    
    const x = Math.min(cropStart.x, currentX);
    const y = Math.min(cropStart.y, currentY);
    const width = Math.abs(currentX - cropStart.x);
    const height = Math.abs(currentY - cropStart.y);
    
    cropSelection = { x, y, width, height };
  }
  
  function handleCanvasMouseUp() {
    isDragging = false;
  }
  
  // Resize handlers
  function openResizeModal() {
    const dims = editor.getDimensions();
    resizeWidth = dims.width;
    resizeHeight = dims.height;
    showResizeModal = true;
  }
  
  function handleWidthChange(e: Event) {
    const input = e.target as HTMLInputElement;
    resizeWidth = parseInt(input.value) || 0;
    if (maintainAspectRatio) {
      resizeHeight = Math.round(resizeWidth / aspectRatio);
    }
  }
  
  function handleHeightChange(e: Event) {
    const input = e.target as HTMLInputElement;
    resizeHeight = parseInt(input.value) || 0;
    if (maintainAspectRatio) {
      resizeWidth = Math.round(resizeHeight * aspectRatio);
    }
  }
  
  function applyResize() {
    if (editor && resizeWidth > 0 && resizeHeight > 0) {
      editor.resize(resizeWidth, resizeHeight);
      updateToolbarState();
    }
    showResizeModal = false;
  }
  
  // Save handlers
  async function handleSave() {
    if (!editor) return;
    
    try {
      const ext = fileName.split('.').pop()?.toLowerCase() || 'png';
      const mimeType = ext === 'jpg' || ext === 'jpeg' ? 'image/jpeg' : 
                       ext === 'webp' ? 'image/webp' : 'image/png';
      
      const blob = await editor.toBlob(mimeType, 0.92);
      dispatch('save', { blob, fileName });
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to save image';
    }
  }
  
  function handleCancel() {
    dispatch('cancel');
  }
</script>

<div class="flex flex-col h-full bg-base-100">
  <!-- Toolbar -->
  <div class="flex items-center gap-2 p-3 border-b border-base-300 flex-wrap">
    <div class="flex items-center gap-1">
      <button class="btn btn-ghost btn-sm" on:click={handleRotateCw} title="Rotate 90° clockwise">
        <RotateCw size={18} />
      </button>
      <button class="btn btn-ghost btn-sm" on:click={handleRotateCcw} title="Rotate 90° counter-clockwise">
        <RotateCcw size={18} />
      </button>
    </div>
    
    <div class="divider divider-horizontal"></div>
    
    <div class="flex items-center gap-1">
      <button class="btn btn-ghost btn-sm" on:click={handleFlipH} title="Flip horizontal">
        <FlipHorizontal size={18} />
      </button>
      <button class="btn btn-ghost btn-sm" on:click={handleFlipV} title="Flip vertical">
        <FlipVertical size={18} />
      </button>
    </div>
    
    <div class="divider divider-horizontal"></div>
    
    <div class="flex items-center gap-1">
      <button class="btn btn-ghost btn-sm" on:click={openResizeModal} title="Resize">
        <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m21 21-6-6m6 6v-4.8m0 4.8h-4.8"/><path d="M3 16.2V21m0 0h4.8M3 21l6-6"/><path d="M21 7.8V3m0 0h-4.8M21 3l-6 6"/><path d="M3 7.8V3m0 0h4.8M3 3l6 6"/></svg>
      </button>
      <button class="btn btn-ghost btn-sm" class:btn-active={isCropping} on:click={startCropMode} title="Crop">
        <Scissors size={18} />
      </button>
    </div>
    
    <div class="divider divider-horizontal"></div>
    
    <div class="flex items-center gap-1">
      <button class="btn btn-ghost btn-sm" on:click={handleUndo} disabled={!canUndo} title="Undo">
        <Undo size={18} />
      </button>
      <button class="btn btn-ghost btn-sm" on:click={handleRedo} disabled={!canRedo} title="Redo">
        <Redo size={18} />
      </button>
    </div>
    
    {#if isCropping}
      <div class="flex items-center gap-1 ml-auto">
        <button class="btn btn-ghost btn-sm btn-error" on:click={cancelCrop}>
          <X size={18} />
          Cancel
        </button>
        <button class="btn btn-ghost btn-sm btn-success" on:click={applyCrop} disabled={!cropSelection}>
          <Check size={18} />
          Apply
        </button>
      </div>
    {/if}
  </div>
  
  <!-- Canvas Area -->
  <div class="flex-1 flex items-center justify-center p-4 overflow-auto bg-base-300 relative">
    {#if loading}
      <div class="flex flex-col items-center gap-4">
        <span class="loading loading-spinner loading-lg"></span>
        <p class="text-base-content/60">Loading image...</p>
      </div>
    {:else if error}
      <div class="text-error">
        <p>{error}</p>
      </div>
    {:else}
      <div class="relative">
        <canvas
          bind:this={canvas}
          class="max-w-full max-h-full shadow-lg"
          class:cursor-crosshair={isCropping}
          on:mousedown={handleCanvasMouseDown}
          on:mousemove={handleCanvasMouseMove}
          on:mouseup={handleCanvasMouseUp}
          on:mouseleave={handleCanvasMouseUp}
        />
        
        {#if isCropping && cropSelection}
          <div
            class="absolute border-2 border-primary bg-primary/20 pointer-events-none"
            style="left: {(cropSelection.x / canvas.width) * 100}%; top: {(cropSelection.y / canvas.height) * 100}%; width: {(cropSelection.width / canvas.width) * 100}%; height: {(cropSelection.height / canvas.height) * 100}%"
          />
        {/if}
      </div>
    {/if}
  </div>
  
  <!-- Footer -->
  <div class="flex items-center justify-end gap-2 p-4 border-t border-base-300">
    <button class="btn btn-ghost" on:click={handleCancel}>Cancel</button>
    <button class="btn btn-primary" on:click={handleSave}>
      Save as New...
    </button>
  </div>
</div>

<!-- Resize Modal -->
{#if showResizeModal}
  <div class="modal modal-open">
    <div class="modal-box">
      <h3 class="font-bold text-lg mb-4">Resize Image</h3>
      
      <div class="flex items-center gap-4 mb-4">
        <div class="form-control flex-1">
          <label class="label">
            <span class="label-text">Width (px)</span>
          </label>
          <input
            type="number"
            class="input input-bordered"
            bind:value={resizeWidth}
            on:input={handleWidthChange}
            min="1"
            max="10000"
          />
        </div>
        
        <div class="pt-8">
          <button
            class="btn btn-ghost btn-sm"
            class:btn-active={maintainAspectRatio}
            on:click={() => maintainAspectRatio = !maintainAspectRatio}
            title="Lock aspect ratio"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
          </button>
        </div>
        
        <div class="form-control flex-1">
          <label class="label">
            <span class="label-text">Height (px)</span>
          </label>
          <input
            type="number"
            class="input input-bordered"
            bind:value={resizeHeight}
            on:input={handleHeightChange}
            min="1"
            max="10000"
          />
        </div>
      </div>
      
      <label class="label cursor-pointer justify-start gap-2 mb-4">
        <input type="checkbox" class="checkbox" bind:checked={maintainAspectRatio} />
        <span class="label-text">Maintain aspect ratio</span>
      </label>
      
      <div class="modal-action">
        <button class="btn btn-ghost" on:click={() => showResizeModal = false}>Cancel</button>
        <button class="btn btn-primary" on:click={applyResize}>Apply</button>
      </div>
    </div>
    <div class="modal-backdrop" on:click={() => showResizeModal = false}></div>
  </div>
{/if}
```

**Step 2: Test component build**

```bash
cd frontend
npm run check
```

**Step 3: Commit**

```bash
git add frontend/src/lib/components/editors/ImageEditor.svelte
git commit -m "feat: add ImageEditor Svelte component with toolbar and crop/resize modals"
```

---

## Task 6: Create Image Editor Page Route

**Files:**
- Create: `frontend/src/routes/(app)/files/edit/[id]/+page.svelte`
- Create: `frontend/src/routes/(app)/files/edit/[id]/+page.ts`

**Step 1: Create the page loader**

```typescript
// +page.ts
import type { PageLoad } from './$types';
import { redirect } from '@sveltejs/kit';

export const load: PageLoad = async ({ params, fetch }) => {
  const fileId = params.id;
  
  // Fetch file metadata
  const response = await fetch(`/api/v1/files/${fileId}`);
  
  if (!response.ok) {
    throw redirect(302, '/files');
  }
  
  const file = await response.json();
  
  // Verify it's an image
  if (!file.mime_type?.startsWith('image/')) {
    throw redirect(302, '/files');
  }
  
  return {
    file,
    fileId
  };
};
```

**Step 2: Create the page component**

```svelte
<!-- +page.svelte -->
<script lang="ts">
  import { goto } from '$app/navigation';
  import ImageEditor from '$lib/components/editors/ImageEditor.svelte';
  import { uploadFile } from '$lib/api/files'; // You'll need to create or use existing upload function
  import type { PageData } from './$types';
  
  export let data: PageData;
  
  let saving = false;
  let error: string | null = null;
  
  async function handleSave(event: CustomEvent<{ blob: Blob; fileName: string }>) {
    const { blob, fileName } = event.detail;
    saving = true;
    error = null;
    
    try {
      // Create file from blob
      const file = new File([blob], fileName, { type: blob.type });
      
      // Upload to same folder as original
      const folderId = data.file.parent_folder_id;
      
      // Use existing upload API
      const formData = new FormData();
      formData.append('file', file);
      if (folderId) {
        formData.append('folder_id', folderId);
      }
      
      const response = await fetch('/api/v1/files/upload', {
        method: 'POST',
        body: formData,
        credentials: 'include'
      });
      
      if (!response.ok) {
        throw new Error('Failed to upload edited image');
      }
      
      // Navigate back to files
      goto('/files');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to save image';
      saving = false;
    }
  }
  
  function handleCancel() {
    goto('/files');
  }
</script>

<div class="h-screen flex flex-col">
  <!-- Header -->
  <div class="flex items-center justify-between px-6 py-4 border-b border-base-300 bg-base-100">
    <div>
      <h1 class="text-xl font-semibold">Edit Image</h1>
      <p class="text-sm text-base-content/60">{data.file.name}</p>
    </div>
    
    {#if saving}
      <div class="flex items-center gap-2">
        <span class="loading loading-spinner loading-sm"></span>
        <span class="text-sm">Saving...</span>
      </div>
    {/if}
  </div>
  
  <!-- Error -->
  {#if error}
    <div class="alert alert-error m-4">
      <span>{error}</span>
    </div>
  {/if}
  
  <!-- Editor -->
  <div class="flex-1 overflow-hidden">
    <ImageEditor
      imageUrl={`/api/v1/files/${data.fileId}/download`}
      fileName={data.file.name}
      on:save={handleSave}
      on:cancel={handleCancel}
    />
  </div>
</div>
```

**Step 3: Check routes compile**

```bash
cd frontend
npm run build
```

**Step 4: Commit**

```bash
git add frontend/src/routes/(app)/files/edit/
git commit -m "feat: add image editor page route at /files/edit/[id]"
```

---

## Task 7: Update FilePreviewModal to Route to Image Editor

**Files:**
- Modify: `frontend/src/lib/components/modals/FilePreviewModal.svelte`

**Step 1: Add navigation import and update edit handler**

Add to imports:
```typescript
import { goto } from '$app/navigation';
```

Update handleEdit function (around line 76):
```typescript
function handleEdit() {
  if (!file || !canEdit) return;
  
  // Route based on editor type
  if (capabilities?.editorType === 'image') {
    goto(`/files/edit/${file.id}`);
    dispatch('close');
  } else {
    dispatch('edit', { file });
  }
}
```

**Step 2: Commit**

```bash
git add frontend/src/lib/components/modals/FilePreviewModal.svelte
git commit -m "feat: route image files to image editor from preview modal"
```

---

## Task 8: Update Monaco Editor Language Support for Config Files

**Files:**
- Modify: `frontend/src/lib/components/editors/TextEditor.svelte` (or BaseEditor.svelte)

**Step 1: Ensure Monaco is configured for YAML, INI, TOML**

Check existing TextEditor/BaseEditor and add language configuration if needed:

```typescript
// In the editor initialization code
const languageMap: Record<string, string> = {
  'json': 'json',
  'yaml': 'yaml',
  'yml': 'yaml',
  'toml': 'toml',
  'ini': 'ini',
  'conf': 'ini',
  'config': 'ini',
  'env': 'plaintext'
};

// When creating editor model
const ext = fileName.split('.').pop()?.toLowerCase() || '';
const language = languageMap[ext] || 'plaintext';

// Monaco editor options for JSON validation
const editorOptions = {
  ...defaultOptions,
  ...(language === 'json' ? {
    validate: true,
    schemaValidation: 'error'
  } : {})
};
```

**Step 2: Verify language modes are loaded**

If using dynamic imports for Monaco languages, ensure they are loaded:

```typescript
async function loadMonacoLanguage(language: string) {
  const languageModules: Record<string, () => Promise<void>> = {
    'json': () => import('monaco-editor/esm/vs/language/json/json.worker'),
    'yaml': () => import('monaco-editor/esm/vs/basic-languages/yaml/yaml.contribution'),
    'ini': () => import('monaco-editor/esm/vs/basic-languages/ini/ini.contribution'),
    'toml': () => import('monaco-editor/esm/vs/basic-languages/toml/toml.contribution')
  };
  
  if (languageModules[language]) {
    await languageModules[language]();
  }
}
```

**Step 3: Commit**

```bash
git add frontend/src/lib/components/editors/
git commit -m "feat: add Monaco language support for yaml, ini, and toml config files"
```

---

## Task 9: Manual Testing

**Files:** N/A - Manual testing

**Step 1: Test Office file preview**

1. Upload a .docx file
2. Click to preview
3. Verify Office icon and "Download to View" button appear
4. Click download button - verify file downloads

**Step 2: Test config file editing**

1. Upload a .json file
2. Preview should show syntax-highlighted code
3. Click Edit - Monaco editor should open with JSON validation
4. Make edit, save - verify file updates

**Step 3: Test YAML editing**

1. Upload a .yaml file
2. Edit should open Monaco with YAML highlighting
3. Verify syntax errors are highlighted

**Step 4: Test image editing**

1. Upload a .jpg image
2. Preview image
3. Click Edit - ImageEditor should open
4. Test each operation:
   - Rotate clockwise/counter-clockwise
   - Flip horizontal/vertical
   - Resize (test aspect ratio lock)
   - Crop (drag selection, apply, cancel)
   - Undo/redo
5. Save as new - verify new file appears in folder

**Step 5: Commit test results documentation**

```bash
git add docs/plans/2026-04-06-extended-file-formats-plan.md
git commit -m "docs: mark extended file formats plan as tested"
```

---

## Summary

This implementation plan adds:

1. **Office file preview** - Metadata display with download button
2. **Image editing** - Full canvas-based editor with rotate, flip, resize, crop
3. **Config file editing** - Monaco editor with language support for JSON, YAML, INI, TOML

All features are client-side, requiring no backend changes beyond existing file upload endpoints.
