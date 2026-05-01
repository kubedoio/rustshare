# Contract: Editor Renderer

```typescript
type RichMarkdownDocumentTarget = {
  documentId?: string;
  moduleKey?: string;
  rootPath?: string;
  sourcePath: string;
  metadataPath?: string;
  attachmentsPath?: string;
  mode?: 'read' | 'edit';
  allowRawMarkdown?: boolean;
};
```

Module pages should not implement their own editors. They call `MarkdownDocumentPage` or route to it.

Examples: NotesModulePage, DecisionsModulePage, MeetingNotesModulePage, KanbanCardDetail and Brainstorming README all use the shared editor.

If rich editor fails, show read mode or raw Markdown fallback and never overwrite automatically.
