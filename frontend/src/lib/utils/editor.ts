/**
 * Editor utility functions for detecting editable file types
 */

export type EditorType = 'text' | 'markdown' | 'excalidraw' | 'image' | 'none';
export type PreviewType = 'image' | 'pdf' | 'video' | 'audio' | 'text' | 'office' | 'code' | 'none';

export interface FileCapabilities {
  editorType: EditorType;
  previewType: PreviewType;
  language?: string;
  canEdit: boolean;
}

// Import the isOfficeFile function from format.ts
import { isOfficeFile } from './format';

/**
 * Detects file capabilities based on name and MIME type
 */
export function detectFileCapabilities(fileName: string, mimeType: string): FileCapabilities {
  if (!fileName || !mimeType) {
    return { editorType: 'none', previewType: 'none', canEdit: false };
  }
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

/**
 * Detects the appropriate editor type for a file based on its name and MIME type
 * @deprecated Use detectFileCapabilities instead for more comprehensive file handling
 */
export function detectEditorType(fileName: string, mimeType: string): EditorType {
  const capabilities = detectFileCapabilities(fileName, mimeType);
  return capabilities.editorType;
}

/**
 * Checks if a file is a known code file based on extension
 */
function isCodeFile(fileName: string): boolean {
  const codeExtensions = [
    // JavaScript/TypeScript
    '.js',
    '.mjs',
    '.cjs',
    '.ts',
    '.tsx',
    '.jsx',
    // Python
    '.py',
    '.pyi',
    '.pyw',
    // Rust
    '.rs',
    // Go
    '.go',
    // Java
    '.java',
    // C/C++
    '.c',
    '.cpp',
    '.cc',
    '.cxx',
    '.h',
    '.hpp',
    '.hxx',
    // C#
    '.cs',
    // PHP
    '.php',
    // Ruby
    '.rb',
    // Swift
    '.swift',
    // Kotlin
    '.kt',
    '.kts',
    // Scala
    '.scala',
    // R
    '.r',
    // Objective-C
    '.m',
    '.mm',
    // Shell scripts
    '.sh',
    '.bash',
    '.zsh',
    '.fish',
    '.ps1',
    '.bat',
    '.cmd',
    // Web
    '.html',
    '.htm',
    '.css',
    '.scss',
    '.sass',
    '.less',
    '.vue',
    '.svelte',
    // Data/config
    '.json',
    '.yaml',
    '.yml',
    '.toml',
    '.xml',
    '.ini',
    '.conf',
    '.config',
    '.properties',
    '.env',
    // SQL
    '.sql',
    // Documentation
    '.txt',
    '.rst',
    '.adoc',
    // Other
    '.dockerfile',
    'makefile',
    '.cmake',
    '.gradle',
    '.gitignore',
    '.gitattributes',
    '.editorconfig',
    '.lock',
    '.log',
    '.csv',
    '.tsv',
    // Graphics code
    '.svg',
    '.glsl',
    '.vert',
    '.frag'
  ];

  return codeExtensions.some((ext) => fileName.toLowerCase().endsWith(ext));
}

/**
 * Gets the Monaco editor language for a file based on its extension
 */
export function getMonacoLanguage(fileName: string): string {
  const ext = fileName.toLowerCase().split('.').pop() || '';

  const languageMap: Record<string, string> = {
    // JavaScript/TypeScript
    js: 'javascript',
    mjs: 'javascript',
    cjs: 'javascript',
    ts: 'typescript',
    tsx: 'typescript',
    jsx: 'javascript',
    // Python
    py: 'python',
    pyi: 'python',
    // Rust
    rs: 'rust',
    // Go
    go: 'go',
    // Java
    java: 'java',
    // C/C++
    c: 'c',
    cpp: 'cpp',
    cc: 'cpp',
    cxx: 'cpp',
    h: 'cpp',
    hpp: 'cpp',
    hxx: 'cpp',
    // C#
    cs: 'csharp',
    // PHP
    php: 'php',
    // Ruby
    rb: 'ruby',
    // Swift
    swift: 'swift',
    // Kotlin
    kt: 'kotlin',
    kts: 'kotlin',
    // Shell
    sh: 'shell',
    bash: 'shell',
    zsh: 'shell',
    fish: 'shell',
    ps1: 'powershell',
    // Web
    html: 'html',
    htm: 'html',
    css: 'css',
    scss: 'scss',
    sass: 'sass',
    less: 'less',
    vue: 'html',
    svelte: 'html',
    // Data/config
    json: 'json',
    yaml: 'yaml',
    yml: 'yaml',
    toml: 'toml',
    xml: 'xml',
    ini: 'ini',
    // SQL
    sql: 'sql',
    // Documentation
    md: 'markdown',
    mdx: 'markdown',
    txt: 'plaintext',
    rst: 'restructuredtext',
    // Other
    dockerfile: 'dockerfile',
    cmake: 'cmake',
    gradle: 'groovy',
    // Graphics
    svg: 'xml',
    glsl: 'glsl',
    vert: 'glsl',
    frag: 'glsl'
  };

  // Handle special filenames without extensions
  const lowerName = fileName.toLowerCase();
  if (lowerName === 'dockerfile' || lowerName.endsWith('dockerfile')) {
    return 'dockerfile';
  }
  if (lowerName === 'makefile' || lowerName.endsWith('makefile')) {
    return 'makefile';
  }
  if (lowerName.startsWith('.git')) {
    return 'plaintext';
  }

  return languageMap[ext] || 'plaintext';
}

/**
 * Maximum file size for editing (10MB)
 */
export const MAX_EDITABLE_SIZE = 10 * 1024 * 1024;

/**
 * Checks if a file can be edited based on its size
 */
export function canEditFileSize(size: number): boolean {
  return size <= MAX_EDITABLE_SIZE;
}

/**
 * Formats file size for display
 */
export function formatEditableSizeLimit(): string {
  return '10 MB';
}
