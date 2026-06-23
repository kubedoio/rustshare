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
import * as monaco from 'monaco-editor';

const monacoLanguages = monaco.languages.getLanguages();

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
		json: 'json',
		yaml: 'yaml',
		yml: 'yaml',
		toml: 'toml',
		ini: 'ini',
		conf: 'ini',
		config: 'ini',
		env: 'plaintext'
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
 * Checks if a file is a known code file based on Monaco's registered languages
 */
function isCodeFile(fileName: string): boolean {
	const lowerName = fileName.toLowerCase();
	return monacoLanguages.some(
		(lang) =>
			lang.extensions?.some((ext) => lowerName.endsWith(ext.toLowerCase())) ||
			lang.filenames?.some((filename) => lowerName === filename.toLowerCase())
	);
}

/**
 * Gets the Monaco editor language for a file based on its extension
 */
export function getMonacoLanguage(fileName: string): string {
	const lowerName = fileName.toLowerCase();

	const byFilename = monacoLanguages.find((lang) =>
		lang.filenames?.some((filename) => lowerName === filename.toLowerCase())
	);
	if (byFilename) return byFilename.id;

	const ext = lowerName.split('.').pop() || '';
	const byExt = monacoLanguages.find((lang) =>
		lang.extensions?.some((e) => ext === e.replace(/^\./, '').toLowerCase())
	);
	if (byExt) return byExt.id;

	return 'plaintext';
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
