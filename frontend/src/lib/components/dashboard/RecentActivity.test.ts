import { render, screen } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockDeps = vi.hoisted(() => {
	let state: {
		items: Array<{
			id: string;
			type: string;
			fileName: string;
			timestamp: string;
			artifactId?: string;
			applicationId?: string;
			accessible?: boolean;
			resourceType?: string;
		}>;
		loading: boolean;
		error: string | null;
		hasMore: boolean;
		cursor: { before_timestamp: string; before_id: string } | null;
	} = {
		items: [],
		loading: false,
		error: null,
		hasMore: true,
		cursor: null
	};
	const subscribers = new Set<(s: typeof state) => void>();

	return {
		setState: (newState: typeof state) => {
			state = newState;
			subscribers.forEach((fn) => fn(state));
		},
		subscribe: (fn: (s: typeof state) => void) => {
			fn(state);
			subscribers.add(fn);
			return () => subscribers.delete(fn);
		},
		fetch: vi.fn(),
		loadMore: vi.fn(),
		reset: vi.fn()
	};
});

vi.mock('$lib/stores/activity', () => ({
	serverActivityStore: {
		subscribe: mockDeps.subscribe,
		fetch: mockDeps.fetch,
		loadMore: mockDeps.loadMore,
		reset: mockDeps.reset
	},
	getActivityDisplay: vi.fn((activity) => ({
		icon: null,
		title: activity.type,
		color: '#000'
	})),
	getRelativeTime: vi.fn(() => '2 hours ago'),
	getActivityHref: vi.fn((activity) => {
		if (!activity.artifactId || activity.accessible === false) return null;
		if (activity.applicationId === 'io.elembra.notes') return `/apps/notes/${activity.artifactId}`;
		if (activity.applicationId === 'io.elembra.meetings')
			return `/apps/meetings/${activity.artifactId}`;
		if (activity.applicationId === 'io.elembra.standups')
			return `/apps/standups/${activity.artifactId}`;
		if (activity.applicationId === 'io.elembra.decisions')
			return `/apps/decisions/${activity.artifactId}`;
		if (activity.applicationId === 'io.elembra.brainstorming')
			return `/apps/brainstorming/${activity.artifactId}`;
		if (activity.applicationId === 'io.elembra.kanban') return '/apps/kanban';
		if (activity.applicationId === 'io.elembra.shares')
			return `/apps/shares/${activity.artifactId}`;
		return `/files?preview=${activity.artifactId}`;
	})
}));

vi.mock('$lib/utils/dashboard', () => ({
	getActivityVerb: vi.fn(() => 'created'),
	getUserInitials: vi.fn(() => 'AJ'),
	getApplicationColor: vi.fn(() => ({ color: '#6b7280', bg: 'rgba(107, 114, 128, 0.1)' }))
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

import RecentActivity from './RecentActivity.svelte';

describe('RecentActivity', () => {
	beforeEach(() => {
		mockDeps.setState({
			items: [],
			loading: false,
			error: null,
			hasMore: true,
			cursor: null
		});
		vi.clearAllMocks();
	});

	it('renders loading state initially', () => {
		mockDeps.setState({
			items: [],
			loading: true,
			error: null,
			hasMore: true,
			cursor: null
		});

		render(RecentActivity, {
			props: { userName: 'Alice Johnson' }
		});

		expect(screen.getByLabelText('Recent activity')).toBeTruthy();
		expect(mockDeps.fetch).toHaveBeenCalledWith(6);
	});

	it('renders activity list with items after fetch', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'note_created',
					fileName: 'My Note',
					timestamp: new Date().toISOString(),
					artifactId: 'note-123',
					applicationId: 'io.elembra.notes'
				},
				{
					id: '2',
					type: 'kanban_created',
					fileName: 'My Board',
					timestamp: new Date().toISOString(),
					artifactId: 'board-456',
					applicationId: 'io.elembra.kanban'
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(RecentActivity, {
			props: { userName: 'Alice Johnson' }
		});

		expect(screen.getByText('My Note')).toBeTruthy();
		expect(screen.getByText('My Board')).toBeTruthy();
	});

	it('uses provided artifact names when activity resource name is unknown', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'file_modified',
					fileName: 'Unknown',
					timestamp: new Date().toISOString(),
					artifactId: 'file-123',
					accessible: true
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(RecentActivity, {
			props: {
				userName: 'Alice Johnson',
				nameLookup: new Map([['file-123', 'Resolved File.md']])
			}
		});

		expect(screen.getByText('Resolved File.md')).toBeTruthy();
		expect(screen.queryByText('Unknown')).toBeNull();
		expect(screen.getByRole('link', { name: /open resolved file\.md/i })).toBeTruthy();
	});

	it('renders clickable links for accessible activities', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'note_created',
					fileName: 'My Note',
					timestamp: new Date().toISOString(),
					artifactId: 'note-123',
					applicationId: 'io.elembra.notes',
					accessible: true
				},
				{
					id: '2',
					type: 'kanban_created',
					fileName: 'My Board',
					timestamp: new Date().toISOString(),
					artifactId: 'board-456',
					applicationId: 'io.elembra.kanban',
					accessible: true
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(RecentActivity, {
			props: { userName: 'Alice Johnson' }
		});

		const links = screen.getAllByRole('link');
		expect(links).toHaveLength(2);

		const noteLink = screen.getByRole('link', { name: /open my note/i });
		expect(noteLink.getAttribute('href')).toBe('/apps/notes/note-123');

		const kanbanLink = screen.getByRole('link', { name: /open my board/i });
		expect(kanbanLink.getAttribute('href')).toBe('/apps/kanban');
	});

	it('renders stale items for inaccessible activities', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'file_uploaded',
					fileName: 'revoked.txt',
					timestamp: new Date().toISOString(),
					artifactId: 'file-123',
					accessible: false
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(RecentActivity, {
			props: { userName: 'Alice Johnson' }
		});

		expect(screen.queryByRole('link')).toBeNull();
		expect(screen.getByText('revoked.txt')).toBeTruthy();
	});

	it('renders stale items for activities without artifactId', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'file_uploaded',
					fileName: 'legacy.txt',
					timestamp: new Date().toISOString()
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(RecentActivity, {
			props: { userName: 'Alice Johnson' }
		});

		expect(screen.queryByRole('link')).toBeNull();
		expect(screen.getByText('legacy.txt')).toBeTruthy();
	});

	it('renders the actor and verb as a grammatical sentence', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'note_created',
					fileName: 'My Note',
					timestamp: new Date().toISOString(),
					artifactId: 'note-123',
					applicationId: 'io.elembra.notes'
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		const { container } = render(RecentActivity, {
			props: { userName: 'Alice Johnson' }
		});

		const description = container.querySelector('.activity-description');
		expect(description).toBeTruthy();
		expect(description!.textContent).toContain('You');
		expect(description!.textContent).toContain('created');
		expect(description!.textContent).not.toContain('was');
	});

	it('falls back to a neutral label instead of "Unknown" for unnamed share events', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'share_created',
					fileName: 'Unknown',
					timestamp: new Date().toISOString(),
					artifactId: 'share-123',
					applicationId: 'io.elembra.shares',
					resourceType: 'share',
					accessible: true
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(RecentActivity, {
			props: { userName: 'Alice Johnson', nameLookup: new Map() }
		});

		expect(screen.getByText('A file')).toBeTruthy();
		expect(screen.queryByText('Unknown')).toBeNull();
	});

	it('falls back to "A folder" for unnamed folder share events', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'share_created',
					fileName: 'Unknown',
					timestamp: new Date().toISOString(),
					artifactId: 'share-456',
					applicationId: 'io.elembra.shares',
					resourceType: 'folder',
					accessible: true
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(RecentActivity, {
			props: { userName: 'Alice Johnson', nameLookup: new Map() }
		});

		expect(screen.getByText('A folder')).toBeTruthy();
		expect(screen.queryByText('Unknown')).toBeNull();
	});

	it('renders empty state when no activities', () => {
		mockDeps.setState({
			items: [],
			loading: false,
			error: null,
			hasMore: true,
			cursor: null
		});

		render(RecentActivity, {
			props: { userName: 'Alice Johnson' }
		});

		expect(
			screen.getByText('Activity will appear here as you work in your workspace.')
		).toBeTruthy();
	});

	it('renders error state on fetch failure', () => {
		mockDeps.setState({
			items: [],
			loading: false,
			error: 'Failed to load activity',
			hasMore: true,
			cursor: null
		});

		render(RecentActivity, {
			props: { userName: 'Alice Johnson' }
		});

		expect(screen.getByText('Failed to load activity')).toBeTruthy();
	});

	it('calls fetch on mount with limit 6', () => {
		render(RecentActivity, {
			props: { userName: 'Alice Johnson' }
		});

		expect(mockDeps.fetch).toHaveBeenCalledTimes(1);
		expect(mockDeps.fetch).toHaveBeenCalledWith(6);
	});

	it('navigates to correct module routes', () => {
		mockDeps.setState({
			items: [
				{
					id: 'n1',
					type: 'note_created',
					fileName: 'Note',
					timestamp: new Date().toISOString(),
					artifactId: 'n-id',
					applicationId: 'io.elembra.notes',
					accessible: true
				},
				{
					id: 'm1',
					type: 'meeting_created',
					fileName: 'Meeting',
					timestamp: new Date().toISOString(),
					artifactId: 'm-id',
					applicationId: 'io.elembra.meetings',
					accessible: true
				},
				{
					id: 's1',
					type: 'standup_created',
					fileName: 'Standup',
					timestamp: new Date().toISOString(),
					artifactId: 's-id',
					applicationId: 'io.elembra.standups',
					accessible: true
				},
				{
					id: 'd1',
					type: 'decision_created',
					fileName: 'Decision',
					timestamp: new Date().toISOString(),
					artifactId: 'd-id',
					applicationId: 'io.elembra.decisions',
					accessible: true
				},
				{
					id: 'b1',
					type: 'brainstorm_created',
					fileName: 'Brainstorm',
					timestamp: new Date().toISOString(),
					artifactId: 'b-id',
					applicationId: 'io.elembra.brainstorming',
					accessible: true
				},
				{
					id: 'sh1',
					type: 'share_created',
					fileName: 'Share',
					timestamp: new Date().toISOString(),
					artifactId: 'sh-id',
					applicationId: 'io.elembra.shares',
					accessible: true
				},
				{
					id: 'f1',
					type: 'file_uploaded',
					fileName: 'File',
					timestamp: new Date().toISOString(),
					artifactId: 'f-id',
					accessible: true
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(RecentActivity, {
			props: { userName: 'Alice Johnson' }
		});

		const links = screen.getAllByRole('link');
		expect(links).toHaveLength(7);

		expect(screen.getByRole('link', { name: /open note/i }).getAttribute('href')).toBe(
			'/apps/notes/n-id'
		);
		expect(screen.getByRole('link', { name: /open meeting/i }).getAttribute('href')).toBe(
			'/apps/meetings/m-id'
		);
		expect(screen.getByRole('link', { name: /open standup/i }).getAttribute('href')).toBe(
			'/apps/standups/s-id'
		);
		expect(screen.getByRole('link', { name: /open decision/i }).getAttribute('href')).toBe(
			'/apps/decisions/d-id'
		);
		expect(screen.getByRole('link', { name: /open brainstorm/i }).getAttribute('href')).toBe(
			'/apps/brainstorming/b-id'
		);
		expect(screen.getByRole('link', { name: /open share/i }).getAttribute('href')).toBe(
			'/apps/shares/sh-id'
		);
		expect(screen.getByRole('link', { name: /open file/i }).getAttribute('href')).toBe(
			'/files?preview=f-id'
		);
	});
});
