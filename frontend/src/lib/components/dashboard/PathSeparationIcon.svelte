<script lang="ts">
	let {
		size = 20,
		strokeWidth = 1.75
	}: {
		size?: number;
		strokeWidth?: number;
	} = $props();
</script>

<svg
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width={strokeWidth}
	stroke-linecap="round"
	stroke-linejoin="round"
>
	<!-- Path separation / fork icon — a single path splitting into two -->
	<circle cx="12" cy="4" r="2" />
	<circle cx="6" cy="20" r="2" />
	<circle cx="18" cy="20" r="2" />
	<path d="M12 6v2c0 1.5-.5 2.5-2 3.5L7 13" />
	<path d="M12 6v2c0 1.5.5 2.5 2 3.5l3 1.5" />
</svg>
