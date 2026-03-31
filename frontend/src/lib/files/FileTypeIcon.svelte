<script lang="ts">
	import { 
		Folder, 
		FileText, 
		FileImage, 
		FileVideo, 
		FileAudio, 
		FileCode, 
		FileSpreadsheet, 
		FileJson,
		FileType,
		File,
		FileArchive,
		FileTerminal
	} from 'lucide-svelte';

	export let mimeType: string;
	export let fileName: string;
	export let size: 'sm' | 'md' | 'lg' = 'md';
	export let className: string = '';

	const iconSizes = {
		sm: 16,
		md: 20,
		lg: 24
	};

	$: iconSize = iconSizes[size];

	function getIconComponent() {
		const lowerName = fileName.toLowerCase();
		
		// Special file types
		if (lowerName.endsWith('.excalidraw') || lowerName.endsWith('.excalidraw.json')) return FileImage;
		if (lowerName.endsWith('.drawio') || lowerName.endsWith('.dio')) return FileImage;
		
		// Images
		if (mimeType.startsWith('image/')) return FileImage;
		
		// Videos
		if (mimeType.startsWith('video/')) return FileVideo;
		
		// Audio
		if (mimeType.startsWith('audio/')) return FileAudio;
		
		// PDF
		if (mimeType === 'application/pdf') return FileType;
		
		// Archives
		if (mimeType.includes('zip') || mimeType.includes('archive') || mimeType.includes('compressed') || 
		    lowerName.endsWith('.zip') || lowerName.endsWith('.tar') || lowerName.endsWith('.gz') || lowerName.endsWith('.rar')) {
			return FileArchive;
		}
		
		// Spreadsheets
		if (mimeType.includes('excel') || mimeType.includes('spreadsheet') || mimeType.includes('sheet') ||
		    lowerName.endsWith('.xls') || lowerName.endsWith('.xlsx') || lowerName.endsWith('.csv')) {
			return FileSpreadsheet;
		}
		
		// Code files
		if (mimeType.includes('javascript') || mimeType.includes('typescript') || 
		    mimeType.includes('python') || mimeType.includes('json') || mimeType.includes('xml') ||
		    lowerName.endsWith('.js') || lowerName.endsWith('.ts') || lowerName.endsWith('.jsx') || 
		    lowerName.endsWith('.tsx') || lowerName.endsWith('.py') || lowerName.endsWith('.json') ||
		    lowerName.endsWith('.yaml') || lowerName.endsWith('.yml') || lowerName.endsWith('.xml')) {
			return FileCode;
		}
		
		// Scripts
		if (lowerName.endsWith('.sh') || lowerName.endsWith('.bash') || lowerName.endsWith('.zsh')) {
			return FileTerminal;
		}
		
		// Text documents
		if (mimeType.includes('text') || mimeType.includes('word') || mimeType.includes('document') ||
		    lowerName.endsWith('.txt') || lowerName.endsWith('.md') || lowerName.endsWith('.doc') || 
		    lowerName.endsWith('.docx') || lowerName.endsWith('.rtf')) {
			return FileText;
		}
		
		// Presentations
		if (mimeType.includes('powerpoint') || mimeType.includes('presentation') ||
		    lowerName.endsWith('.ppt') || lowerName.endsWith('.pptx')) {
			return FileText;
		}
		
		return File;
	}

	function getIconColor(): string {
		const lowerName = fileName.toLowerCase();
		
		// Images
		if (mimeType.startsWith('image/')) return 'text-info';
		
		// Videos
		if (mimeType.startsWith('video/')) return 'text-red-400';
		
		// Audio
		if (mimeType.startsWith('audio/')) return 'text-pink-400';
		
		// PDF
		if (mimeType === 'application/pdf') return 'text-red-500';
		
		// Archives
		if (mimeType.includes('zip') || mimeType.includes('archive') || mimeType.includes('compressed')) {
			return 'text-yellow-400';
		}
		
		// Spreadsheets
		if (mimeType.includes('excel') || mimeType.includes('spreadsheet') || mimeType.includes('sheet')) {
			return 'text-green-400';
		}
		
		// Code
		if (mimeType.includes('javascript') || mimeType.includes('typescript') || 
		    mimeType.includes('python') || mimeType.includes('json') || mimeType.includes('xml') ||
		    lowerName.endsWith('.js') || lowerName.endsWith('.ts') || lowerName.endsWith('.py')) {
			return 'text-blue-400';
		}
		
		// Documents
		if (mimeType.includes('text') || mimeType.includes('word') || mimeType.includes('document')) {
			return 'text-blue-400';
		}
		
		return 'text-base-content/50';
	}

	$: IconComponent = getIconComponent();
	$: iconColor = getIconColor();
</script>

<div class="{className} {iconColor}">
	<IconComponent size={iconSize} />
</div>
