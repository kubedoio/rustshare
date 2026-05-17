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
	<!-- Four squares inside a square — Windows-style grid -->
	<rect x="2" y="2" width="20" height="20" rx="2" />
	<line x1="12" y1="2" x2="12" y2="22" />
	<line x1="2" y1="12" x2="22" y2="12" />
</svg>
