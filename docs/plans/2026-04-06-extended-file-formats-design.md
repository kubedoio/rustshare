# Extended File Format Support Design

## Overview

Add enhanced support for MS Office formats (metadata preview), image editing (basic operations + crop), and config file editing (JSON/YAML/etc.) — all client-side to maintain lightweight architecture.

## Goals

- Enable preview and edit capabilities for additional file formats
- Keep implementation client-side (no server-side conversion services)
- Maintain consistency with existing file handling patterns
- Prevent accidental data loss with safe save defaults

## Supported Formats

### MS Office Formats

**File Types:** `.docx`, `.doc`, `.xlsx`, `.xls`, `.pptx`, `.ppt`

**MIME Types:**
- `application/vnd.openxmlformats-officedocument.wordprocessingml.document`
- `application/msword`
- `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet`
- `application/vnd.ms-excel`
- `application/vnd.openxmlformats-officedocument.presentationml.presentation`
- `application/vnd.ms-powerpoint`

**Preview Mode:**
- Display file metadata (name, size, type, created/modified dates)
- Show Office-specific icon (Word, Excel, PowerPoint)
- Display thumbnail if available (from existing thumbnail service)
- Show "Download to view" message with prominent download button
- No server-side conversion

**Edit Mode:** Not applicable — Office files remain view-only

---

### Image Formats

**File Types:** `.jpg`, `.jpeg`, `.png`, `.webp`, `.gif`

**MIME Types:** `image/jpeg`, `image/png`, `image/webp`, `image/gif`

**Preview Mode:**
- Existing image viewer (no changes)
- Full-size image display with zoom/pan

**Edit Mode:**
Canvas-based editor with the following operations:

| Operation | Description |
|-----------|-------------|
| Rotate ↻ | 90° clockwise |
| Rotate ↺ | 90° counter-clockwise |
| Flip ↔ | Horizontal flip |
| Flip ↕ | Vertical flip |
| Resize | Percentage or pixel dimensions (aspect ratio lock) |
| Crop ✂ | Rectangular selection with drag handles |
| Undo/Redo | History stack for all operations |

**Crop Interaction:**
1. Click "Crop" button enters crop mode
2. Drag to create selection rectangle
3. Drag handles to adjust selection
4. Click "Apply" or press Enter to apply crop
5. Click outside or "Cancel" to abort

**Save Behavior:**
- Default: "Save as New" (prompts for new filename)
- Optional: "Overwrite Original" (requires explicit confirmation)
- Prevents accidental data loss

---

### Config File Formats

**File Types:** `.json`, `.yaml`, `.yml`, `.conf`, `.config`, `.ini`, `.toml`, `.env`

**Preview Mode:**
- Syntax-highlighted view in Monaco editor (read-only)
- Collapsible sections for JSON/YAML

**Edit Mode:**
Full Monaco editor with:

| Feature | JSON | YAML | INI/CONF | TOML |
|---------|------|------|----------|------|
| Syntax Highlighting | ✅ | ✅ | ✅ | ✅ |
| Validation | ✅ Schema | ✅ Syntax | ❌ | ❌ |
| Formatting | ✅ Prettify/Minify | ✅ | ❌ | ❌ |
| Line Numbers | ✅ | ✅ | ✅ | ✅ |
| Minimap | ✅ | ✅ | ✅ | ✅ |
| Code Folding | ✅ | ✅ | ✅ | ✅ |

---

## File Type Registry

Centralized registry for determining file capabilities:

```typescript
interface FileTypeConfig {
  mimeTypes: string[];
  extensions: string[];
  canPreview: boolean;
  canEdit: boolean;
  previewComponent: 'image' | 'pdf' | 'text' | 'office' | 'code' | 'none';
  editorComponent: 'text' | 'markdown' | 'excalidraw' | 'image' | 'none';
  editorLanguage?: string; // Monaco language ID
}
```

### Registry Entries

| Category | Extensions | Preview | Edit | Editor Language |
|----------|------------|---------|------|-----------------|
| Office Word | .docx, .doc | office | none | - |
| Office Excel | .xlsx, .xls | office | none | - |
| Office PowerPoint | .pptx, .ppt | office | none | - |
| Image | .jpg, .png, .webp, .gif | image | image | - |
| JSON | .json | code | text | json |
| YAML | .yaml, .yml | code | text | yaml |
| INI/Config | .conf, .config, .ini | code | text | ini |
| TOML | .toml | code | text | toml |
| Environment | .env | code | text | plaintext |

---

## UI/UX Design

### File Preview Modal Updates

Add detection for new file types and appropriate preview components:

```
┌─────────────────────────────────────────────┐
│  filename.ext                    [X]        │
│  2.5 MB • image/png                         │
├─────────────────────────────────────────────┤
│                                             │
│         [Preview Content Area]              │
│                                             │
├─────────────────────────────────────────────┤
│  [Edit] [Download] [Close]                  │
└─────────────────────────────────────────────┘
```

**Office File Preview:**
```
┌─────────────────────────────────────────────┐
│         [Word/Excel/PowerPoint Icon]        │
│                                             │
│         document.docx                       │
│         Microsoft Word Document             │
│         2.5 MB • Modified yesterday         │
│                                             │
│         [Download to View]                  │
└─────────────────────────────────────────────┘
```

### Image Editor Layout

```
┌─────────────────────────────────────────────────────────┐
│  Image Editor                              [X]          │
│  vacation.jpg (3.2 MB)                                  │
├─────────────────────────────────────────────────────────┤
│  [↻ Rotate] [↺ Rotate] [↔ Flip] [↕ Flip] [✂ Crop]      │
│  [Resize...] [↩ Undo] [↪ Redo]                         │
├─────────────────────────────────────────────────────────┤
│                                                         │
│              ┌─────────────────────┐                    │
│              │                     │                    │
│              │    [Canvas with     │                    │
│              │   selection when    │                    │
│              │    cropping]        │                    │
│              │                     │                    │
│              └─────────────────────┘                    │
│                                                         │
├─────────────────────────────────────────────────────────┤
│  [Cancel]        [Save as New...]                       │
└─────────────────────────────────────────────────────────┘
```

---

## Technical Implementation

### Image Editor (Canvas-Based)

```typescript
class ImageEditor {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private history: ImageData[];
  private cropSelection: { x, y, width, height } | null;
  
  // Operations
  rotate(degrees: 90 | -90): void;
  flip(direction: 'horizontal' | 'vertical'): void;
  resize(width: number, height: number): void;
  applyCrop(): void;
  
  // History
  undo(): void;
  redo(): void;
  
  // Export
  toBlob(): Promise<Blob>;
}
```

### File Type Detection

Update existing `detectEditorType` to return more granular types:

```typescript
type EditorType = 'text' | 'markdown' | 'excalidraw' | 'image' | 'none';
type PreviewType = 'image' | 'pdf' | 'video' | 'audio' | 'text' | 'office' | 'code' | 'none';

function detectFileCapabilities(fileName: string, mimeType: string): {
  editorType: EditorType;
  previewType: PreviewType;
  language?: string;
};
```

### Monaco Language Configuration

Ensure Monaco is configured for new languages:

```typescript
// Monaco language modes to register
const MONACO_LANGUAGES = [
  'json',
  'yaml',
  'ini',
  'toml',
  'plaintext' // for .env files
];
```

---

## Data Flow

### Opening a File

1. User clicks file in FileGrid/FileList
2. `FilePreviewModal` opens with file info
3. Detect file type from registry
4. Load preview content:
   - Images/Video/Audio: Use preview URL
   - Text/Code: Fetch content via API
   - Office: Display metadata only
5. Render appropriate preview component
6. Show "Edit" button if `canEdit` is true

### Editing Flow

1. User clicks "Edit" button
2. Navigate to editor route with file ID
3. Load file content via API
4. Initialize appropriate editor component:
   - Text/Code: Monaco editor
   - Image: Canvas-based image editor
   - Markdown: Markdown editor (existing)
   - Excalidraw: Excalidraw editor (existing)
5. User makes edits
6. Save:
   - Generate new file blob
   - Upload via existing file upload API
   - Navigate back to file location

### Image Editing Flow

1. Load image into canvas at original resolution
2. Apply operations to canvas context
3. Maintain history stack of ImageData
4. On save:
   - Convert canvas to Blob
   - Create File object
   - Upload to server
   - Refresh file list

---

## Error Handling

| Scenario | User Feedback |
|----------|---------------|
| Image too large for canvas | "Image is too large to edit. Consider resizing first." |
| Invalid JSON on save | "Invalid JSON syntax. Please fix errors before saving." |
| YAML syntax error | Underline in editor + tooltip with error message |
| Network error on save | Toast notification with retry option |
| Canvas memory error | "Browser ran out of memory. Try with a smaller image." |

---

## Performance Considerations

1. **Image Editor:**
   - Canvas operations are GPU-accelerated in modern browsers
   - Large images (>10MP) may cause memory issues
   - Consider max dimension limit (e.g., 4096px)

2. **Monaco Editor:**
   - Lazy-load language modes on demand
   - Dispose editor instances when navigating away

3. **File Loading:**
   - Continue using existing preview URL pattern
   - No additional server load for previews

---

## Accessibility

1. **Image Editor:**
   - All toolbar buttons have aria-labels
   - Keyboard shortcuts for operations (Ctrl+Z for undo, etc.)
   - Focus management for crop mode

2. **Code Editor:**
   - Monaco has built-in accessibility support
   - Ensure proper ARIA labels for custom controls

---

## Testing Strategy

1. **Unit Tests:**
   - File type detection logic
   - Image editor operations (rotate, flip, crop)
   - History stack management

2. **Integration Tests:**
   - File preview modal with different file types
   - Edit → Save flow
   - Upload new file after editing

3. **Manual Testing:**
   - Large image handling
   - Various Office file formats
   - Edge cases (corrupted files, empty files)

---

## Migration Notes

- No database migrations required
- No backend API changes required (uses existing upload endpoints)
- Purely frontend feature addition
- Backward compatible — existing files continue to work

---

## Future Extensions

If server-side capabilities are added later:
- Office file conversion to PDF for preview
- Advanced image processing (filters, adjustments)
- Collaborative editing for config files
