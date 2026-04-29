<script lang="ts">
	interface Props {
		disabled?: boolean;
		multiple?: boolean;
		onFilesSelected?: (files: globalThis.File[]) => void;
	}

	let { disabled = false, multiple = true, onFilesSelected = () => {} }: Props = $props();

	let fileInput: HTMLInputElement | undefined = $state();

	function handleClick() {
		fileInput?.click();
	}

	function handleFileChange(event: Event) {
		const target = event.target as HTMLInputElement;
		const files = target.files;

		if (files && files.length > 0) {
			onFilesSelected(Array.from(files));
			// Reset input so same file can be selected again
			target.value = '';
		}
	}
</script>

<input
	id="upload-file-input"
	bind:this={fileInput}
	type="file"
	class="hidden"
	{multiple}
	onchange={handleFileChange}
	{disabled}
/>

<button class="btn btn-sm btn-primary lg:btn-md" onclick={handleClick} {disabled}>
	<svg
		xmlns="http://www.w3.org/2000/svg"
		fill="none"
		viewBox="0 0 24 24"
		stroke-width="1.5"
		stroke="currentColor"
		class="h-4 w-4 lg:h-5 lg:w-5"
	>
		<path
			stroke-linecap="round"
			stroke-linejoin="round"
			d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5"
		/>
	</svg>
	<span class="hidden sm:inline">Upload</span>
</button>
