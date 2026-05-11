import { describe, it, expect } from 'vitest'
import {
	parseCardMarkdown,
	serializeCardMarkdown,
	cardDetailToMarkdown,
	markdownToCardDetail,
} from './cardMarkdown'
import type { KanbanCardDetail } from '$lib/api/types'

const exampleCard = `---
id: card-001
title: "Task title"
board: product-launch
column: 01-ready
priority: medium
labels:
  - id: lbl-001
    name: Medium
    color: yellow
assignees:
  - id: user-001
    display_name: Melise Colak
    initials: MC
    avatar_url: null
position: 1000
created_at: 2026-05-11T18:53:00Z
updated_at: 2026-05-11T18:53:00Z
attachments:
  - id: att-001
    name: requirements.pdf
    path: attachments/requirements.pdf
    mime_type: application/pdf
    size: 248233
    created_by: user-001
    created_at: 2026-05-11T18:53:00Z
checklists:
  - id: chk-001
    title: Checklist
    items:
      - id: item-001
        text: Define requirements
        done: true
activity:
  - id: act-001
    event_type: card_moved
    actor: user-001
    text: Moved this card from Backlog to Ready
    timestamp: 2026-05-11T18:53:00Z
---

## Description

Add a more detailed description here.

## Comments

Some comment here.
`

describe('parseCardMarkdown', () => {
	it('parses a basic card with frontmatter and description', () => {
		const card = parseCardMarkdown(exampleCard)

		expect(card.id).toBe('card-001')
		expect(card.title).toBe('Task title')
		expect(card.board).toBe('product-launch')
		expect(card.column).toBe('01-ready')
		expect(card.priority).toBe('medium')
		expect(card.labels).toHaveLength(1)
		expect(card.labels[0]).toEqual({
			id: 'lbl-001',
			name: 'Medium',
			color: 'yellow',
		})
		expect(card.assignees).toHaveLength(1)
		expect(card.assignees[0]).toEqual({
			id: 'user-001',
			display_name: 'Melise Colak',
			initials: 'MC',
			avatar_url: null,
		})
		expect(card.position).toBe(1000)
		expect(card.createdAt).toBe('2026-05-11T18:53:00Z')
		expect(card.updatedAt).toBe('2026-05-11T18:53:00Z')
		expect(card.attachments).toHaveLength(1)
		expect(card.attachments[0].name).toBe('requirements.pdf')
		expect(card.checklists).toHaveLength(1)
		expect(card.checklists[0].items).toHaveLength(1)
		expect(card.activity).toHaveLength(1)
		expect(card.description).toBe('Add a more detailed description here.')
	})

	it('parses a card with empty description section', () => {
		const raw = `---
id: card-empty
title: Empty Description
---

## Description

## Other Section

Content here.
`
		const card = parseCardMarkdown(raw)
		expect(card.description).toBe('')
	})

	it('parses a card with no description section', () => {
		const raw = `---
id: card-no-desc
title: No Description Section
---

This is just body content without a description header.
`
		const card = parseCardMarkdown(raw)
		expect(card.description).toBe('This is just body content without a description header.')
	})

	it('parses a card with special characters in description', () => {
		const raw = `---
id: card-special
title: Special Characters
---

## Description

| Col1 | Col2 |
|------|------|
| A    | B    |

\`\`\`yaml
key: value
\`\`\`

> Blockquote with "quotes" and 'apostrophes'
`
		const card = parseCardMarkdown(raw)
		expect(card.description).toContain('| Col1 | Col2 |')
		expect(card.description).toContain('```yaml')
		expect(card.description).toContain('Blockquote')
	})

	it('provides sensible defaults for missing fields', () => {
		const raw = `---
id: minimal
title: Minimal
---

## Description

Hello.
`
		const card = parseCardMarkdown(raw)
		expect(card.id).toBe('minimal')
		expect(card.title).toBe('Minimal')
		expect(card.board).toBe('')
		expect(card.column).toBe('')
		expect(card.priority).toBeUndefined()
		expect(card.labels).toEqual([])
		expect(card.assignees).toEqual([])
		expect(card.position).toBe(0)
		expect(card.createdAt).toBe('')
		expect(card.updatedAt).toBe('')
		expect(card.attachments).toEqual([])
		expect(card.checklists).toEqual([])
		expect(card.activity).toEqual([])
	})

	it('handles malformed input gracefully', () => {
		const card = parseCardMarkdown('not a valid frontmatter file at all')
		expect(card.id).toBe('')
		expect(card.title).toBe('')
		expect(card.description).toBe('not a valid frontmatter file at all')
	})

	it('extracts description when it is not the first section', () => {
		const raw = `---
id: card-order
title: Section Order
---

## Comments

First comment.

## Description

The actual description.

## Notes

Some notes.
`
		const card = parseCardMarkdown(raw)
		expect(card.description).toBe('The actual description.')
	})

	it('stops description at the next ## section', () => {
		const raw = `---
id: card-multi
title: Multiple Sections
---

## Description

Desc here.

## Comments

Comment one.

## Tasks

Task one.
`
		const card = parseCardMarkdown(raw)
		expect(card.description).toBe('Desc here.')
	})
})

describe('serializeCardMarkdown', () => {
	it('serializes a basic card', () => {
		const card = parseCardMarkdown(exampleCard)
		const serialized = serializeCardMarkdown(card)
		expect(serialized).toContain('---')
		expect(serialized).toContain('id: card-001')
		expect(serialized).toContain('title: Task title')
		expect(serialized).toContain('## Description')
		expect(serialized).toContain('Add a more detailed description here.')
	})

	it('updates updatedAt when not provided', () => {
		const card = parseCardMarkdown(exampleCard)
		card.updatedAt = ''
		const serialized = serializeCardMarkdown(card)
		const reparsed = parseCardMarkdown(serialized)
		expect(reparsed.updatedAt).not.toBe('')
		expect(reparsed.updatedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/)
	})

	it('omits undefined priority', () => {
		const card = parseCardMarkdown(exampleCard)
		card.priority = undefined
		const serialized = serializeCardMarkdown(card)
		const reparsed = parseCardMarkdown(serialized)
		expect(reparsed.priority).toBeUndefined()
	})
})

describe('round-trip', () => {
	it('parse -> serialize -> parse produces equal results', () => {
		const original = parseCardMarkdown(exampleCard)
		const serialized = serializeCardMarkdown(original)
		const reparsed = parseCardMarkdown(serialized)

		expect(reparsed.id).toBe(original.id)
		expect(reparsed.title).toBe(original.title)
		expect(reparsed.board).toBe(original.board)
		expect(reparsed.column).toBe(original.column)
		expect(reparsed.priority).toBe(original.priority)
		expect(reparsed.labels).toEqual(original.labels)
		expect(reparsed.assignees).toEqual(original.assignees)
		expect(reparsed.position).toBe(original.position)
		expect(reparsed.createdAt).toBe(original.createdAt)
		expect(reparsed.attachments).toEqual(original.attachments)
		expect(reparsed.checklists).toEqual(original.checklists)
		expect(reparsed.activity).toEqual(original.activity)
		expect(reparsed.description).toBe(original.description)
	})

	it('preserves unknown frontmatter fields through round-trip', () => {
		const raw = `---
id: card-unknown
title: Unknown Fields
custom_field: hello
nested:
  key: value
---

## Description

Body.
`
		const card = parseCardMarkdown(raw)
		expect(card.custom_field).toBe('hello')
		expect(card.nested).toEqual({ key: 'value' })

		const serialized = serializeCardMarkdown(card)
		const reparsed = parseCardMarkdown(serialized)

		expect(reparsed.custom_field).toBe('hello')
		expect(reparsed.nested).toEqual({ key: 'value' })
	})
})

describe('cardDetailToMarkdown', () => {
	it('converts KanbanCardDetail to KanbanCardMarkdown', () => {
		const detail: KanbanCardDetail = {
			id: 'card-002',
			title: 'API Card',
			slug: 'api-card',
			content: 'Detailed content',
			description_preview: 'Detailed content',
			column_id: 'col-001',
			status: 'active',
			order: 42,
			labels: [],
			assignees: [],
			due_date: null,
			priority: 'high',
			attachments_count: 0,
			checklist: { done: 0, total: 0 },
			checklists: [],
			archived: false,
			created_at: '2026-01-01T00:00:00Z',
			updated_at: '2026-01-02T00:00:00Z',
			path: 'boards/test/card-002.md',
			schema_version: '1.0',
			attachments: [],
			activity: [],
		}

		const markdown = cardDetailToMarkdown(detail)
		expect(markdown.id).toBe('card-002')
		expect(markdown.title).toBe('API Card')
		expect(markdown.column).toBe('col-001')
		expect(markdown.position).toBe(42)
		expect(markdown.priority).toBe('high')
		expect(markdown.description).toBe('Detailed content')
		expect(markdown.createdAt).toBe('2026-01-01T00:00:00Z')
		expect(markdown.updatedAt).toBe('2026-01-02T00:00:00Z')
	})

	it('maps normal priority to undefined', () => {
		const detail: KanbanCardDetail = {
			id: 'card-003',
			title: 'Normal Priority',
			slug: 'normal-priority',
			content: '',
			description_preview: '',
			column_id: 'col-001',
			status: 'active',
			order: 0,
			labels: [],
			assignees: [],
			due_date: null,
			priority: 'normal',
			attachments_count: 0,
			checklist: { done: 0, total: 0 },
			checklists: [],
			archived: false,
			created_at: '',
			updated_at: '',
			path: '',
			schema_version: '1.0',
			attachments: [],
			activity: [],
		}

		const markdown = cardDetailToMarkdown(detail)
		expect(markdown.priority).toBeUndefined()
	})
})

describe('markdownToCardDetail', () => {
	it('converts KanbanCardMarkdown to KanbanCardDetail', () => {
		const markdown = parseCardMarkdown(exampleCard)
		const detail = markdownToCardDetail(markdown, {
			slug: 'task-title',
			path: 'boards/product-launch/cards/card-001.md',
		})

		expect(detail.id).toBe('card-001')
		expect(detail.title).toBe('Task title')
		expect(detail.content).toBe('Add a more detailed description here.')
		expect(detail.description_preview).toBe('Add a more detailed description here.')
		expect(detail.column_id).toBe('01-ready')
		expect(detail.order).toBe(1000)
		expect(detail.priority).toBe('medium')
		expect(detail.slug).toBe('task-title')
		expect(detail.path).toBe('boards/product-launch/cards/card-001.md')
		expect(detail.attachments_count).toBe(1)
		expect(detail.checklist).toEqual({ done: 1, total: 1 })
	})

	it('uses defaults when base is not provided', () => {
		const markdown = parseCardMarkdown(exampleCard)
		const detail = markdownToCardDetail(markdown, {})

		expect(detail.status).toBe('active')
		expect(detail.archived).toBe(false)
		expect(detail.due_date).toBeNull()
		expect(detail.schema_version).toBe('1.0')
	})
})
