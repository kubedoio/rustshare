<script lang="ts">
	import MarkdownDocumentPage from './MarkdownDocumentPage.svelte';
	import type { EditorMode, EditorSaveStatus, EditorPermissions } from '../types';
	import type { RichMarkdownAttachment } from '../types';

	interface Props {
		title?: string;
		content?: string;
		color?: string | null;
		mode?: EditorMode;
		permissions?: EditorPermissions;
		saveStatus?: EditorSaveStatus;
		revision?: number | string;
		label?: string;
		breadcrumb?: Array<{ label: string; onClick?: () => void }>;
		metadata?: string;
		attachments?: RichMarkdownAttachment[];
		autosaveDelay?: number;
		showBack?: boolean;
		embedSketchesAsBase64?: boolean;
		collab?: boolean;
		docId?: string;
		showNoteActions?: boolean;
		initialAttachmentsOpen?: boolean;
		subtitle?: string;
		onSave?: (event: CustomEvent<{ content: string; docId?: string }>) => void;
		onRename?: () => void;
	}

	let { onSave = () => {}, onRename = () => {}, ...props }: Props = $props();
</script>

<MarkdownDocumentPage {...props} on:save={onSave} on:rename={onRename} />
