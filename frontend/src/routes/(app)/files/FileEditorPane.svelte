<script lang="ts">
	import type { File } from '$lib/api/types';
	import { TextEditor, MarkdownEditor, ExcalidrawEditor } from '$lib/components/editors';

	interface Props {
		showTextEditor: boolean;
		showMarkdownEditor: boolean;
		showExcalidrawEditor: boolean;
		editorTarget: File | null;
		onEditorClose: () => void;
		onEditorSaved: (event?: { file?: File }) => void;
	}

	let {
		showTextEditor,
		showMarkdownEditor,
		showExcalidrawEditor,
		editorTarget,
		onEditorClose,
		onEditorSaved
	}: Props = $props();
</script>

{#if showTextEditor && editorTarget}
	<TextEditor
		open={showTextEditor}
		file={editorTarget}
		onClose={onEditorClose}
		onSaved={onEditorSaved}
	/>
{/if}

{#if showMarkdownEditor && editorTarget}
	<MarkdownEditor
		open={showMarkdownEditor}
		file={editorTarget}
		onClose={onEditorClose}
		onSaved={onEditorSaved}
	/>
{/if}

{#if showExcalidrawEditor && editorTarget}
	<ExcalidrawEditor
		open={showExcalidrawEditor}
		file={editorTarget}
		onClose={onEditorClose}
		onSaved={onEditorSaved}
	/>
{/if}
