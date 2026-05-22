## Design System
Always read DESIGN.md before making any visual or UI decisions.
All font choices, colors, spacing, and aesthetic direction are defined there.
Do not deviate without explicit user approval.
In QA mode, flag any code that doesn't match DESIGN.md.

## Skill routing

When the user's request matches an available skill, ALWAYS invoke it using the Skill
tool as your FIRST action. Do NOT answer directly, do NOT use other tools first.
The skill has specialized workflows that produce better results than ad-hoc answers.

Key routing rules:
- Product ideas, "is this worth building", brainstorming → invoke office-hours
- Bugs, errors, "why is this broken", 500 errors → invoke investigate
- Ship, deploy, push, create PR → invoke ship
- QA, test the site, find bugs → invoke qa
- Code review, check my diff → invoke review
- Update docs after shipping → invoke document-release
- Weekly retro → invoke retro
- Design system, brand → invoke design-consultation
- Visual audit, design polish → invoke design-review
- Architecture review → invoke plan-eng-review
- Save progress, checkpoint, resume → invoke checkpoint
- Code quality, health check → invoke health
## Design System
Always read DESIGN.md before making any visual or UI decisions.
All font choices, colors, spacing, and aesthetic direction are defined there.
Do not deviate without explicit user approval.
In QA mode, flag any code that doesn't match DESIGN.md.

## Skill routing

When the user's request matches an available skill, ALWAYS invoke it using the Skill
tool as your FIRST action. Do NOT answer directly, do NOT use other tools first.
The skill has specialized workflows that produce better results than ad-hoc answers.

Key routing rules:
- Product ideas, "is this worth building", brainstorming → invoke office-hours
- Bugs, errors, "why is this broken", 500 errors → invoke investigate
- Ship, deploy, push, create PR → invoke ship
- QA, test the site, find bugs → invoke qa
- Code review, check my diff → invoke review
- Update docs after shipping → invoke document-release
- Weekly retro → invoke retro
- Design system, brand → invoke design-consultation
- Visual audit, design polish → invoke design-review
- Architecture review → invoke plan-eng-review
- Save progress, checkpoint, resume → invoke checkpoint
- Code quality, health check → invoke health

## Svelte 5 Reactivity — Critical Anti-Patterns

The frontend uses Svelte 5 runes mode. The following patterns have caused production incidents (`effect_update_depth_exceeded`) and must never be re-introduced.

### 1. NEVER use `$effect` for bidirectional prop ↔ state sync
```svelte
<!-- ❌ WRONG — causes runaway loops -->
$effect(() => { localMarkdown = currentMarkdown; });
```
When the parent updates `currentMarkdown` (e.g. after save), the child overwrites `localMarkdown`, which may dispatch `change`, which updates `currentMarkdown`, which triggers the effect again → 128-depth safety limit hit.

**Correct patterns:**
- One-time init with `untrack()`: `let localMarkdown = $state(untrack(() => currentMarkdown));`
- If reactive sync is truly needed, use a guarded single-write with explicit equality checks and `untrack()` around the write.

### 2. NEVER call editor mutation APIs inside `$effect` without guards
```svelte
<!-- ❌ WRONG — Tiptap/ProseMirror side-effects can trigger callbacks that write state -->
$effect(() => {
    if (editor && initialized) {
        editor.setEditable(editable);
    }
});
```
`setEditable()` emits an `update` event, which can cascade through `onDocumentUpdate` → debounced dispatch → parent state change → child prop change → effect re-runs.

**Correct pattern:**
- Pass `editable` to the editor constructor (`buildEditorOptions`) instead.
- If dynamic toggling is needed, guard with a `lastEditable` snapshot and only call when the value actually changed.

### 3. NEVER leave dead `$state` writes in callbacks
```svelte
<!-- ❌ WRONG — editorTick is never read, but writes still schedule flushes -->
let editorTick = $state(0);
onSelectionUpdate: () => { editorTick++; }
```
Even unread `$state` variables schedule Svelte flushes on every write. In high-frequency callbacks (Tiptap `onSelectionUpdate`, `onTransaction`) this creates unnecessary reactive pressure.

**Correct pattern:**
- Remove dead state entirely, or use a plain variable / closure if the value is only needed imperatively.

### 4. Query `setOptions()` inside `$effect`
`query.setOptions()` creates a new `queryFn` reference every run. TanStack Query may treat this as changed options and trigger a refetch. While not a loop by itself, it causes double-fetching on mount. Prefer stable options or memoized `queryFn` references.

### Reference incident
- **File:** `frontend/src/lib/editor/components/RichMarkdownEditor.svelte`
- **Root cause:** `$effect(() => editor.setEditable(editable))` + dead `editorTick` state + bidirectional `localMarkdown` sync effect
- **Fix commit:** `8d6071c` (removed redundant effect, removed dead state, replaced bidirectional sync with `untrack()` init)
