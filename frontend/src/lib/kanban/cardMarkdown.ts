import matter from 'gray-matter'
import * as yaml from 'js-yaml'
import type {
	KanbanCardDetail,
	KanbanLabel,
	KanbanAssignee,
	KanbanCardAttachment,
	KanbanChecklistGroup,
	KanbanEvent,
} from '$lib/api/types'

export interface KanbanCardMarkdown {
	id: string
	title: string
	board: string
	column: string
	priority?: 'low' | 'medium' | 'high' | 'urgent'
	labels: KanbanLabel[]
	assignees: KanbanAssignee[]
	position: number
	createdAt: string
	updatedAt: string
	attachments: KanbanCardAttachment[]
	checklists: KanbanChecklistGroup[]
	activity: KanbanEvent[]
	dueDate?: string
	description: string
	// Allow preserving unknown frontmatter fields
	[key: string]: unknown
}

interface FrontmatterData {
	id?: string
	title?: string
	board?: string
	column?: string
	priority?: 'low' | 'medium' | 'high' | 'urgent'
	labels?: KanbanLabel[]
	assignees?: KanbanAssignee[]
	position?: number
	createdAt?: string
	created_at?: string
	updatedAt?: string
	updated_at?: string
	attachments?: KanbanCardAttachment[]
	checklists?: KanbanChecklistGroup[]
	activity?: KanbanEvent[]
	dueDate?: string
	due_date?: string
	[key: string]: unknown
}

const DESCRIPTION_REGEX = /^## Description\s*\n?([\s\S]*?)(?=^## |(?![\s\S]))/im

function extractDescription(body: string): string {
	const match = body.match(DESCRIPTION_REGEX)
	if (match) {
		return match[1].trim()
	}
	return body.trim()
}

function computeChecklist(checklists: KanbanChecklistGroup[]) {
	let done = 0
	let total = 0
	for (const group of checklists) {
		for (const item of group.items) {
			total++
			if (item.done) {
				done++
			}
		}
	}
	return { done, total }
}

/**
 * Parse a Kanban card Markdown file.
 * Extracts YAML frontmatter and the ## Description section.
 * Preserves unknown frontmatter keys in the returned object.
 */
export function parseCardMarkdown(raw: string): KanbanCardMarkdown {
	try {
		const parsed = matter(raw, {
			engines: {
				yaml: {
					parse: (str: string) => yaml.load(str, { schema: yaml.JSON_SCHEMA }) as object,
					stringify: (data: unknown) => yaml.dump(data, { lineWidth: -1 }),
				},
			},
		})
		const data = (parsed.data as FrontmatterData | undefined) || {}

		const description = extractDescription(parsed.content)

		return {
			...data,
			id: data.id ?? '',
			title: data.title ?? '',
			board: data.board ?? '',
			column: data.column ?? '',
			priority: data.priority,
			labels: data.labels ?? [],
			assignees: data.assignees ?? [],
			position: data.position ?? 0,
			createdAt: data.createdAt ?? data.created_at ?? '',
			updatedAt: data.updatedAt ?? data.updated_at ?? '',
			attachments: data.attachments ?? [],
			checklists: data.checklists ?? [],
			activity: data.activity ?? [],
			dueDate: data.dueDate ?? data.due_date ?? '',
			description,
		}
	} catch {
		return {
			id: '',
			title: '',
			board: '',
			column: '',
			labels: [],
			assignees: [],
			position: 0,
			createdAt: '',
			updatedAt: '',
			attachments: [],
			checklists: [],
			activity: [],
			description: raw,
		}
	}
}

/**
 * Serialize a Kanban card to Markdown format.
 * Writes YAML frontmatter + ## Description section.
 * Preserves any extra keys from the input object.
 */
export function serializeCardMarkdown(card: KanbanCardMarkdown): string {
	const {
		id,
		title,
		board,
		column,
		priority,
		labels,
		assignees,
		position,
		createdAt,
		updatedAt,
		attachments,
		checklists,
		activity,
		dueDate,
		description,
		...unknown
	} = card

	const frontmatter = {
		id,
		title,
		board,
		column,
		...(priority !== undefined && { priority }),
		labels,
		assignees,
		position,
		created_at: createdAt,
		updated_at: updatedAt ?? new Date().toISOString(),
		attachments,
		checklists,
		activity,
		...(dueDate && { due_date: dueDate }),
		...unknown,
	}

	const yamlStr = yaml.dump(frontmatter, { lineWidth: -1 })
	const body = `## Description\n\n${description}`

	return `---\n${yamlStr}---\n\n${body}\n`
}

/**
 * Convert from the existing API type (KanbanCardDetail) to KanbanCardMarkdown.
 * This is a convenience adapter.
 */
export function cardDetailToMarkdown(card: KanbanCardDetail): KanbanCardMarkdown {
	return {
		id: card.id,
		title: card.title,
		board: '',
		column: card.column_id,
		priority: card.priority === 'normal' ? undefined : card.priority,
		labels: card.labels,
		assignees: card.assignees,
		position: card.order,
		createdAt: card.created_at,
		updatedAt: card.updated_at,
		attachments: card.attachments,
		checklists: card.checklists,
		activity: card.activity,
		dueDate: card.due_date ?? '',
		description: card.content,
	}
}

/**
 * Convert from KanbanCardMarkdown to a format suitable for the existing API.
 */
export function markdownToCardDetail(
	markdown: KanbanCardMarkdown,
	base: Partial<KanbanCardDetail>,
): KanbanCardDetail {
	const checklist = computeChecklist(markdown.checklists)

	return {
		...base,
		id: markdown.id,
		title: markdown.title,
		slug: base.slug ?? (markdown.slug as string) ?? '',
		content: markdown.description,
		description_preview: markdown.description.slice(0, 200),
		column_id: markdown.column,
		status: base.status ?? 'active',
		order: markdown.position,
		labels: markdown.labels,
		assignees: markdown.assignees,
		due_date: markdown.dueDate?.trim() ? markdown.dueDate : (base.due_date ?? null),
		priority: markdown.priority ?? 'normal',
		attachments_count: markdown.attachments?.length ?? 0,
		checklist,
		checklists: markdown.checklists,
		archived: base.archived ?? false,
		created_at: markdown.createdAt,
		updated_at: markdown.updatedAt,
		path: base.path ?? '',
		schema_version: base.schema_version ?? '1.0',
		attachments: markdown.attachments,
		activity: markdown.activity,
	} as KanbanCardDetail
}
