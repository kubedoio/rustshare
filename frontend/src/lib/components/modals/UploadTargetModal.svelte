<script lang="ts">
	import { Upload } from 'lucide-svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import FolderTreePicker from './FolderTreePicker.svelte';

	interface Props {
		open?: boolean;
		currentFolderId?: string | null;
		onClose?: () => void;
		onConfirm?: (payload: { targetFolderId: string | null }) => void;
	}

	let {
		open = false,
		currentFolderId = null,
		onClose = () => {},
		onConfirm = () => {}
	}: Props = $props();

	let selectedFolderId: string | null = $state(null);

	function handleSubmit() {
		onConfirm({ targetFolderId: selectedFolderId });
	}

	function handleClose() {
		selectedFolderId = currentFolderId;
		onClose();
	}

	$effect(() => {
		if (open) {
			selectedFolderId = currentFolderId;
		}
	});
</script>

<ModalBase {open} title="Upload Files" onClose={handleClose}>
	<p class="mb-4 text-sm text-base-content/60">Select destination folder for your upload</p>

	<!-- Location Section -->
	<div>
		<label class="mb-2 block text-sm font-medium text-base-content/80">
			Target Folder
			<FolderTreePicker
				{selectedFolderId}
				{currentFolderId}
				onSelect={(id) => (selectedFolderId = id)}
			/>
		</label>
	</div>

	<!-- Actions -->
	<div class="mt-6 flex justify-end gap-3">
		<button
			type="button"
			class="rounded-lg px-4 py-2 text-sm font-medium text-base-content/70 transition-colors hover:bg-base-200 hover:text-base-content"
			onclick={handleClose}
		>
			Cancel
		</button>
		<button
			type="button"
			class="flex items-center gap-2 rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-600"
			onclick={handleSubmit}
		>
			<Upload size={16} />
			Select & Upload
		</button>
	</div>
</ModalBase>
