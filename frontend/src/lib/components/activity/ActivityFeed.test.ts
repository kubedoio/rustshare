import { render, screen, fireEvent } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockDeps = vi.hoisted(() => {
	let state: {
		items: Array<{
			id: string;
			type: string;
			fileName: string;
			timestamp: string;
			artifactId?: string;
			moduleKey?: string;
			accessible?: boolean;
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
		description: `Description for ${activity.fileName}`,
		color: '#000'
	})),
	getRelativeTime: vi.fn(() => '2 hours ago'),
	getActivityHref: vi.fn((activity) => {
		if (!activity.artifactId || activity.accessible === false) return null;
		if (activity.moduleKey === 'notes') return `/modules/notes/${activity.artifactId}`;
		return `/files?preview=${activity.artifactId}`;
	})
}));

import ActivityFeed from './ActivityFeed.svelte';

describe('ActivityFeed', () => {
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

		render(ActivityFeed, {
			props: { maxItems: 10, showHeader: true }
		});

		expect(screen.getByText('Recent Activity')).toBeTruthy();
		expect(mockDeps.fetch).toHaveBeenCalledWith(10);
	});

	it('renders activity list after fetch', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'file_uploaded',
					fileName: 'document.pdf',
					timestamp: new Date().toISOString(),
					artifactId: 'file-1',
					accessible: true
				},
				{
					id: '2',
					type: 'note_modified',
					fileName: 'My Note',
					timestamp: new Date().toISOString(),
					artifactId: 'note-1',
					moduleKey: 'notes',
					accessible: true
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(ActivityFeed, {
			props: { maxItems: 10, showHeader: true }
		});

		expect(screen.getByText('Description for document.pdf')).toBeTruthy();
		expect(screen.getByText('Description for My Note')).toBeTruthy();
	});

	it('renders empty state when no activities', () => {
		mockDeps.setState({
			items: [],
			loading: false,
			error: null,
			hasMore: true,
			cursor: null
		});

		render(ActivityFeed, {
			props: { maxItems: 10, showHeader: true }
		});

		expect(screen.getByText('No recent activity')).toBeTruthy();
	});

	it('renders error state on fetch failure', () => {
		mockDeps.setState({
			items: [],
			loading: false,
			error: 'Failed to load activity',
			hasMore: true,
			cursor: null
		});

		render(ActivityFeed, {
			props: { maxItems: 10, showHeader: true }
		});

		expect(screen.getByText('Failed to load activity')).toBeTruthy();
	});

	it('supports load more pagination', async () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'file_uploaded',
					fileName: 'first.pdf',
					timestamp: new Date().toISOString(),
					artifactId: 'file-1',
					accessible: true
				}
			],
			loading: false,
			error: null,
			hasMore: true,
			cursor: null
		});

		render(ActivityFeed, {
			props: { maxItems: 10, showHeader: true }
		});

		const loadMoreButton = screen.getByRole('button', { name: /load more/i });
		expect(loadMoreButton).toBeTruthy();

		await fireEvent.click(loadMoreButton);
		expect(mockDeps.loadMore).toHaveBeenCalledWith(10);
	});

	it('does not show load more button when hasMore is false', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'file_uploaded',
					fileName: 'only.pdf',
					timestamp: new Date().toISOString(),
					artifactId: 'file-1',
					accessible: true
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(ActivityFeed, {
			props: { maxItems: 10, showHeader: true }
		});

		expect(screen.queryByRole('button', { name: /load more/i })).toBeNull();
	});

	it('renders accessible items as clickable links', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'note_modified',
					fileName: 'My Note',
					timestamp: new Date().toISOString(),
					artifactId: 'note-1',
					moduleKey: 'notes',
					accessible: true
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(ActivityFeed, {
			props: { maxItems: 10, showHeader: true }
		});

		const link = screen.getByRole('link');
		expect(link.getAttribute('href')).toBe('/modules/notes/note-1');
	});

	it('renders inaccessible items without links', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'file_uploaded',
					fileName: 'revoked.pdf',
					timestamp: new Date().toISOString(),
					artifactId: 'file-1',
					accessible: false
				}
			],
			loading: false,
			error: null,
			hasMore: false,
			cursor: null
		});

		render(ActivityFeed, {
			props: { maxItems: 10, showHeader: true }
		});

		expect(screen.queryByRole('link')).toBeNull();
		expect(screen.getByText('Description for revoked.pdf')).toBeTruthy();
	});

	it('hides header when showHeader is false', () => {
		mockDeps.setState({
			items: [],
			loading: false,
			error: null,
			hasMore: true,
			cursor: null
		});

		render(ActivityFeed, {
			props: { maxItems: 10, showHeader: false }
		});

		expect(screen.queryByText('Recent Activity')).toBeNull();
	});

	it('disables load more button while loading', () => {
		mockDeps.setState({
			items: [
				{
					id: '1',
					type: 'file_uploaded',
					fileName: 'first.pdf',
					timestamp: new Date().toISOString(),
					artifactId: 'file-1',
					accessible: true
				}
			],
			loading: true,
			error: null,
			hasMore: true,
			cursor: null
		});

		render(ActivityFeed, {
			props: { maxItems: 10, showHeader: true }
		});

		const button = screen.getByRole('button');
		expect(button.hasAttribute('disabled')).toBe(true);
	});
});
