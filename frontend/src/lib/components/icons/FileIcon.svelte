<script lang="ts">
	export let mimeType: string;
	export let size: 'sm' | 'md' | 'lg' = 'md';
	export let iconClass: string = '';

	$: normalized = mimeType.toLowerCase();
	$: isImage = normalized.startsWith('image/');
	$: isVideo = normalized.startsWith('video/');
	$: isAudio = normalized.startsWith('audio/');
	$: isPdf = normalized.includes('pdf');
	$: isArchive =
		normalized.includes('zip') ||
		normalized.includes('tar') ||
		normalized.includes('gzip') ||
		normalized.includes('archive');
	$: isWord = normalized.includes('msword') || normalized.includes('wordprocessingml');
	$: isExcel = normalized.includes('ms-excel') || normalized.includes('spreadsheetml');
	$: isPowerPoint = normalized.includes('ms-powerpoint') || normalized.includes('presentationml');
	$: isText = normalized.startsWith('text/');

	const sizeClasses = {
		sm: 'w-4 h-4',
		md: 'w-5 h-5',
		lg: 'w-8 h-8'
	};

	$: sizeClass = sizeClasses[size];
</script>

{#if isImage}
	<!-- Image icon -->
	<svg
		class="{sizeClass} {iconClass}"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
		<circle cx="9" cy="9" r="2" />
		<path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
	</svg>
{:else if isVideo}
	<!-- Video icon -->
	<svg
		class="{sizeClass} {iconClass}"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<path d="m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5" />
		<rect x="2" y="6" width="14" height="12" rx="2" />
	</svg>
{:else if isAudio}
	<!-- Audio icon -->
	<svg
		class="{sizeClass} {iconClass}"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<path d="M9 18V5l12-2v13" />
		<circle cx="6" cy="18" r="3" />
		<circle cx="18" cy="16" r="3" />
	</svg>
{:else if isPdf}
	<!-- PDF icon -->
	<svg
		class="{sizeClass} {iconClass}"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
		<polyline points="14 2 14 8 20 8" />
		<path d="M10 13v-1a2 2 0 0 1 2-2h0a2 2 0 0 1 2 2v1" />
		<path d="M8 17h8" />
		<path d="M8 13h8" />
	</svg>
{:else if isArchive}
	<!-- Archive icon -->
	<svg
		class="{sizeClass} {iconClass}"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<path d="M21 8v13H3V8" />
		<path d="M1 3h22v5H1z" />
		<path d="M10 12h4" />
	</svg>
{:else if isWord}
	<!-- Word document icon -->
	<svg
		class="{sizeClass} {iconClass}"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
		<polyline points="14 2 14 8 20 8" />
		<path d="M8 13h2" />
		<path d="M8 17h2" />
		<path d="M12 13h4" />
		<path d="M12 17h4" />
	</svg>
{:else if isExcel}
	<!-- Spreadsheet icon -->
	<svg
		class="{sizeClass} {iconClass}"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
		<polyline points="14 2 14 8 20 8" />
		<path d="M8 13h2" />
		<path d="M8 17h2" />
		<path d="M12 13h4" />
		<path d="M12 17h4" />
		<path d="M8 9h8" />
	</svg>
{:else if isPowerPoint}
	<!-- Presentation icon -->
	<svg
		class="{sizeClass} {iconClass}"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
		<polyline points="14 2 14 8 20 8" />
		<path d="M9 12l2-1 2 1-2 1-2-1z" />
		<path d="M9 15l2-1 2 1-2 1-2-1z" />
	</svg>
{:else if isText}
	<!-- Text file icon -->
	<svg
		class="{sizeClass} {iconClass}"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
		<polyline points="14 2 14 8 20 8" />
		<line x1="16" x2="8" y1="13" y2="13" />
		<line x1="16" x2="8" y1="17" y2="17" />
		<line x1="10" x2="8" y1="9" y2="9" />
	</svg>
{:else}
	<!-- Default file icon -->
	<svg
		class="{sizeClass} {iconClass}"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.5"
		stroke-linecap="round"
		stroke-linejoin="round"
	>
		<path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
		<polyline points="14 2 14 8 20 8" />
	</svg>
{/if}
