import { render, screen } from '@testing-library/svelte';
import { describe, it, expect, vi } from 'vitest';
import RecentActivity from './RecentActivity.svelte';

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$lib/stores/activity', () => ({
	getActivityDisplay: vi.fn((activity) => ({
		icon: null,
		title: activity.type,
		color: '#000'
	})),
	getRelativeTime: vi.fn(() => '2 hours ago')
}));

vi.mock('$lib/utils/dashboard', () => ({
	getActivityVerb: vi.fn(() => 'was created'),
	getUserInitials: vi.fn(() => 'AJ')
}));

describe('RecentActivity', () => {
	const mockActivities = [
		{
			id: '1',
			type: 'note_created' as const,
			fileName: 'My Note',
			timestamp: new Date().toISOString(),
			artifactId: 'note-123',
			moduleKey: 'notes'
		},
		{
			id: '2',
			type: 'kanban_created' as const,
			fileName: 'My Board',
			timestamp: new Date().toISOString(),
			artifactId: 'board-456',
			moduleKey: 'kanban'
		},
		{
			id: '3',
			type: 'file_uploaded' as const,
			fileName: 'legacy.txt',
			timestamp: new Date().toISOString()
		}
	];

	it('renders activity list with items', () => {
		render(RecentActivity, {
			props: {
				activities: mockActivities,
				userName: 'Alice Johnson'
			}
		});

		expect(screen.getByText('My Note')).toBeTruthy();
		expect(screen.getByText('My Board')).toBeTruthy();
		expect(screen.getByText('legacy.txt')).toBeTruthy();
	});

	it('renders clickable links for activities with artifactId and moduleKey', () => {
		render(RecentActivity, {
			props: {
				activities: mockActivities,
				userName: 'Alice Johnson'
			}
		});

		const links = screen.getAllByRole('link');
		expect(links).toHaveLength(2);

		const noteLink = screen.getByRole('link', { name: /open my note/i });
		expect(noteLink.getAttribute('href')).toBe('/modules/notes/note-123');

		const kanbanLink = screen.getByRole('link', { name: /open my board/i });
		expect(kanbanLink.getAttribute('href')).toBe('/modules/kanban');
	});

	it('renders stale items for legacy activities without artifactId', () => {
		render(RecentActivity, {
			props: {
				activities: [mockActivities[2]],
				userName: 'Alice Johnson'
			}
		});

		expect(screen.queryByRole('link')).toBeNull();
		expect(screen.getByText('legacy.txt')).toBeTruthy();
	});

	it('navigates to correct module routes', () => {
		const routeTestActivities = [
			{
				id: 'n1',
				type: 'note_created' as const,
				fileName: 'Note',
				timestamp: new Date().toISOString(),
				artifactId: 'n-id',
				moduleKey: 'notes'
			},
			{
				id: 'm1',
				type: 'meeting_created' as const,
				fileName: 'Meeting',
				timestamp: new Date().toISOString(),
				artifactId: 'm-id',
				moduleKey: 'meetings'
			},
			{
				id: 's1',
				type: 'standup_created' as const,
				fileName: 'Standup',
				timestamp: new Date().toISOString(),
				artifactId: 's-id',
				moduleKey: 'standups'
			},
			{
				id: 'd1',
				type: 'decision_created' as const,
				fileName: 'Decision',
				timestamp: new Date().toISOString(),
				artifactId: 'd-id',
				moduleKey: 'decisions'
			},
			{
				id: 'b1',
				type: 'brainstorm_created' as const,
				fileName: 'Brainstorm',
				timestamp: new Date().toISOString(),
				artifactId: 'b-id',
				moduleKey: 'brainstorming'
			},
			{
				id: 'k1',
				type: 'kanban_created' as const,
				fileName: 'Kanban',
				timestamp: new Date().toISOString(),
				artifactId: 'k-id',
				moduleKey: 'kanban'
			},
			{
				id: 'sh1',
				type: 'share_created' as const,
				fileName: 'Share',
				timestamp: new Date().toISOString(),
				artifactId: 'sh-id',
				moduleKey: 'shares'
			},
			{
				id: 'f1',
				type: 'file_uploaded' as const,
				fileName: 'File',
				timestamp: new Date().toISOString(),
				artifactId: 'f-id',
				moduleKey: undefined
			}
		];

		render(RecentActivity, {
			props: {
				activities: routeTestActivities,
				userName: 'Alice Johnson'
			}
		});

		const links = screen.getAllByRole('link');
		expect(links).toHaveLength(8);

		expect(screen.getByRole('link', { name: /open note/i }).getAttribute('href')).toBe(
			'/modules/notes/n-id'
		);
		expect(screen.getByRole('link', { name: /open meeting/i }).getAttribute('href')).toBe(
			'/modules/meetings/m-id'
		);
		expect(screen.getByRole('link', { name: /open standup/i }).getAttribute('href')).toBe(
			'/modules/standups/s-id'
		);
		expect(screen.getByRole('link', { name: /open decision/i }).getAttribute('href')).toBe(
			'/modules/decisions/d-id'
		);
		expect(screen.getByRole('link', { name: /open brainstorm/i }).getAttribute('href')).toBe(
			'/modules/brainstorming/b-id'
		);
		expect(screen.getByRole('link', { name: /open kanban/i }).getAttribute('href')).toBe(
			'/modules/kanban'
		);
		expect(screen.getByRole('link', { name: /open share/i }).getAttribute('href')).toBe(
			'/modules/shares/sh-id'
		);
		expect(screen.getByRole('link', { name: /open file/i }).getAttribute('href')).toBe(
			'/files?preview=f-id'
		);
	});
});
