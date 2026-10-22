import { act, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import MeetingActions from './MeetingActions.svelte';
import { ApiError } from '$lib/api/types';
import type { MeetingAction } from '$lib/api/meetings';

const mocks = vi.hoisted(() => ({
	listActions: vi.fn(),
	createAction: vi.fn(),
	updateAction: vi.fn(),
	listActionAssignees: vi.fn(),
	listDecisions: vi.fn(),
	toast: vi.fn(),
	authValue: { id: 'user-1', tenant_id: 'tenant-1' }
}));
vi.mock('$lib/api/meetings', () => ({ meetingsApi: mocks }));
vi.mock('$lib/stores/toast', () => ({ toastStore: { show: mocks.toast } }));
vi.mock('$lib/stores/auth', () => ({
	currentUser: {
		subscribe(run: (user: typeof mocks.authValue) => void) {
			run(mocks.authValue);
			return () => {};
		}
	}
}));

const action = (overrides: Partial<MeetingAction> = {}): MeetingAction => ({
	id: 'action-1',
	meeting_id: 'meeting-1',
	calendar_event_id: 'event-1',
	title: 'Publish minutes',
	description: 'Review with the team.',
	assignee_id: 'person-1',
	assignee: { id: 'person-1', display_name: 'Alex' },
	due_date: '2026-11-01',
	state: 'open',
	version: 1,
	created_at: '2026-10-21T10:00:00Z',
	updated_at: '2026-10-21T10:00:00Z',
	created_by: 'user-1',
	updated_by: 'user-1',
	completed_at: null,
	completed_by: null,
	related_decision_available: true,
	related_decision: {
		id: 'decision-1',
		title: 'Approve publication',
		open_url: '/apps/decisions/decision-1'
	},
	...overrides
});
const page = (items: MeetingAction[], page = 1, total = items.length) => ({
	items,
	page,
	total,
	per_page: 50
});

describe('MeetingActions', () => {
	beforeEach(() => {
		vi.resetAllMocks();
		sessionStorage.clear();
		mocks.authValue = { id: 'user-1', tenant_id: 'tenant-1' };
		mocks.listActions.mockResolvedValue(page([action()]));
		mocks.createAction.mockResolvedValue(action());
		mocks.updateAction.mockResolvedValue(action({ version: 2 }));
		mocks.listActionAssignees.mockResolvedValue({
			items: [{ id: 'person-1', display_name: 'Alex' }],
			total: 1,
			page: 1,
			per_page: 50
		});
		mocks.listDecisions.mockResolvedValue({
			items: [{ id: 'link-1', available: true, decision: action().related_decision }],
			total: 1,
			page: 1,
			per_page: 50
		});
	});

	it('shows historical completion, date-only due dates and authorized Decision navigation', async () => {
		mocks.listActions.mockResolvedValue(page([action({ state: 'completed' })]));
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await screen.findByRole('heading', { name: 'Publish minutes' });
		expect(screen.getByText(/Completed · Responsible: Alex/)).toBeTruthy();
		expect(screen.getByText('2026-11-01').getAttribute('datetime')).toBe('2026-11-01');
		expect(screen.getByRole('link', { name: 'Approve publication' }).getAttribute('href')).toBe(
			'/apps/decisions/decision-1'
		);
		expect(screen.getByRole('button', { name: 'Reopen Publish minutes' })).toBeTruthy();
	});

	it('creates with authoritative assignee, calendar day and related Decision references', async () => {
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Create action' }));
		await screen.findByRole('option', { name: 'Alex' });
		await screen.findByRole('option', { name: 'Approve publication' });
		await fireEvent.input(screen.getByLabelText('Action title'), {
			target: { value: 'Send agenda' }
		});
		await fireEvent.input(screen.getByLabelText('Action description'), {
			target: { value: 'Include the proposal.' }
		});
		await fireEvent.change(screen.getByLabelText('Responsible person'), {
			target: { value: 'person-1' }
		});
		await fireEvent.input(screen.getByLabelText('Due date'), { target: { value: '2026-11-01' } });
		await fireEvent.change(screen.getByLabelText('Related Decision'), {
			target: { value: 'decision-1' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save new action' }));
		await waitFor(() =>
			expect(mocks.createAction).toHaveBeenCalledWith('meeting-1', {
				request_id: expect.any(String),
				title: 'Send agenda',
				description: 'Include the proposal.',
				assignee_id: 'person-1',
				due_date: '2026-11-01',
				related_decision_id: 'decision-1'
			})
		);
		expect(screen.getByText(/Assignment does not grant meeting access/)).toBeTruthy();
	});

	it('edits using the observed version and preserves a concealed Decision reference', async () => {
		mocks.listActions.mockResolvedValue(
			page([action({ related_decision_available: false, related_decision: null })])
		);
		mocks.listDecisions.mockResolvedValue({
			items: [{ id: 'hidden', available: false }],
			total: 1,
			page: 1,
			per_page: 50
		});
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(await screen.findByRole('button', { name: 'Edit Publish minutes' }));
		await fireEvent.input(screen.getByLabelText('Action title'), {
			target: { value: 'Publish reviewed minutes' }
		});
		await fireEvent.input(screen.getByLabelText('Due date'), { target: { value: '2026-11-03' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Save action' }));
		await waitFor(() => expect(mocks.updateAction).toHaveBeenCalled());
		expect(mocks.updateAction.mock.calls[0][2]).toMatchObject({
			expected_version: 1,
			title: 'Publish reviewed minutes',
			due_date: '2026-11-03'
		});
		expect(mocks.updateAction.mock.calls[0][2]).not.toHaveProperty('related_decision_id');
		expect(screen.queryByRole('link')).toBeNull();
		expect(screen.queryByText('decision-1')).toBeNull();
	});

	it('completes and reopens with fresh versions without deleting historical actions', async () => {
		let current = action();
		mocks.listActions.mockImplementation(async () => page([current]));
		mocks.updateAction.mockImplementation(
			async (_meeting: string, _id: string, request: { state: MeetingAction['state'] }) => {
				current = { ...current, state: request.state, version: current.version + 1 };
				return current;
			}
		);
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(await screen.findByRole('button', { name: 'Complete Publish minutes' }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Reopen Publish minutes' }));
		await screen.findByRole('button', { name: 'Complete Publish minutes' });
		expect(
			mocks.updateAction.mock.calls.map((call) => [call[2].expected_version, call[2].state])
		).toEqual([
			[1, 'completed'],
			[2, 'open']
		]);
		expect(mocks.updateAction.mock.calls[0][2]).not.toHaveProperty('related_decision_id');
		expect(screen.getByRole('heading', { name: 'Publish minutes' })).toBeTruthy();
	});

	it('recovers the same creation request after an interrupted response and component restart', async () => {
		mocks.createAction.mockRejectedValueOnce(new Error('interrupted'));
		const first = render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Create action' }));
		await fireEvent.input(screen.getByLabelText('Action title'), {
			target: { value: 'Recover this action' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save new action' }));
		await screen.findByRole('button', { name: 'Retry creation' });
		const request = mocks.createAction.mock.calls[0][1];
		expect(sessionStorage.getItem('meeting-action-request:tenant-1:user-1:meeting-1')).toBe(
			JSON.stringify(request)
		);
		first.unmount();
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Create action' }));
		expect((screen.getByLabelText('Action title') as HTMLInputElement).value).toBe(
			'Recover this action'
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Retry creation' }));
		await waitFor(() => expect(mocks.createAction).toHaveBeenCalledTimes(2));
		expect(mocks.createAction.mock.calls[1]).toEqual(['meeting-1', request]);
		await waitFor(() =>
			expect(sessionStorage.getItem('meeting-action-request:tenant-1:user-1:meeting-1')).toBeNull()
		);
	});

	it('isolates recovered drafts by tenant, user and meeting', async () => {
		sessionStorage.setItem(
			'meeting-action-request:tenant-1:user-1:meeting-1',
			JSON.stringify({
				request_id: 'request-1',
				title: 'Private draft',
				description: 'Private text',
				assignee_id: null,
				due_date: null,
				related_decision_id: null
			})
		);
		mocks.authValue = { id: 'user-2', tenant_id: 'tenant-1' };
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Create action' }));
		expect((screen.getByLabelText('Action title') as HTMLInputElement).value).toBe('');
		expect(screen.queryByText('Private text')).toBeNull();
	});

	it('retries an ambiguous update with the original request identity and body', async () => {
		mocks.updateAction.mockRejectedValueOnce(new Error('interrupted'));
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(await screen.findByRole('button', { name: 'Complete Publish minutes' }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Retry change' }));
		await waitFor(() => expect(mocks.updateAction).toHaveBeenCalledTimes(2));
		expect(mocks.updateAction.mock.calls[1]).toEqual(mocks.updateAction.mock.calls[0]);
	});

	it('refreshes on a stale version instead of overwriting another update', async () => {
		mocks.updateAction.mockRejectedValueOnce(new ApiError(409, 'Conflict'));
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(await screen.findByRole('button', { name: 'Complete Publish minutes' }));
		await screen.findByText('This action changed. Review the latest version before editing again.');
		expect(mocks.listActions).toHaveBeenCalledTimes(2);
		expect(screen.queryByRole('button', { name: 'Retry change' })).toBeNull();
		expect(mocks.updateAction).toHaveBeenCalledTimes(1);
	});

	it('ignores old list responses after switching meetings', async () => {
		let resolveOld: (value: unknown) => void = () => {};
		mocks.listActions.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveOld = resolve;
				})
		);
		mocks.listActions.mockResolvedValueOnce(page([]));
		const view = render(MeetingActions, { props: { meetingId: 'old-meeting' } });
		await waitFor(() => expect(mocks.listActions).toHaveBeenCalledWith('old-meeting', 1));
		await view.rerender({ meetingId: 'new-meeting' });
		await screen.findByText('No actions yet.');
		await act(() => resolveOld(page([action({ title: 'Old private action' })])));
		expect(screen.queryByText('Old private action')).toBeNull();
	});

	it('loads bounded additional pages and ignores an older page after a mutation refresh', async () => {
		const first = action();
		const second = action({ id: 'action-2', title: 'Second action' });
		let resolveOld: (value: unknown) => void = () => {};
		mocks.listActions.mockResolvedValueOnce(page([first], 1, 2));
		mocks.listActions.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveOld = resolve;
				})
		);
		mocks.listActions.mockResolvedValueOnce(page([first, second]));
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(await screen.findByRole('button', { name: 'Load more actions' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Complete Publish minutes' }));
		await screen.findByRole('heading', { name: 'Second action' });
		await act(() => resolveOld(page([second], 2, 2)));
		expect(screen.getAllByRole('heading', { name: 'Second action' })).toHaveLength(1);
		expect(mocks.listActions).toHaveBeenCalledWith('meeting-1', 2);
	});

	it('rejects an oversized UTF-8 description before creating a request', async () => {
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Create action' }));
		expect(screen.getByLabelText('Action title').getAttribute('maxlength')).toBe('255');
		await fireEvent.input(screen.getByLabelText('Action title'), { target: { value: 'Review' } });
		await fireEvent.input(screen.getByLabelText('Action description'), {
			target: { value: '😀'.repeat(4097) }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save new action' }));
		await screen.findByText('Action description must be at most 16 KiB.');
		expect(mocks.createAction).not.toHaveBeenCalled();
	});

	it('unlocks a rejected create draft for correction with a fresh request identity', async () => {
		mocks.createAction.mockRejectedValueOnce(new ApiError(400, 'Invalid assignee'));
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Create action' }));
		await fireEvent.input(screen.getByLabelText('Action title'), {
			target: { value: 'Retained draft' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save new action' }));
		await screen.findByText('Action details were rejected. Check the details and try again.');
		expect((screen.getByLabelText('Action title') as HTMLInputElement).value).toBe(
			'Retained draft'
		);
		expect((screen.getByLabelText('Action title') as HTMLInputElement).disabled).toBe(false);
		await fireEvent.input(screen.getByLabelText('Action title'), {
			target: { value: 'Corrected draft' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save new action' }));
		await waitFor(() => expect(mocks.createAction).toHaveBeenCalledTimes(2));
		expect(mocks.createAction.mock.calls[1][1].title).toBe('Corrected draft');
		expect(mocks.createAction.mock.calls[1][1].request_id).not.toBe(
			mocks.createAction.mock.calls[0][1].request_id
		);
	});

	it('unlocks a rejected update while preserving its visible draft', async () => {
		mocks.updateAction.mockRejectedValueOnce(new ApiError(400, 'Invalid assignee'));
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(await screen.findByRole('button', { name: 'Edit Publish minutes' }));
		await fireEvent.input(screen.getByLabelText('Action title'), {
			target: { value: 'Retained edit' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save action' }));
		await screen.findByText('Action details were rejected. Check the details and try again.');
		expect((screen.getByLabelText('Action title') as HTMLInputElement).value).toBe('Retained edit');
		await fireEvent.input(screen.getByLabelText('Action title'), {
			target: { value: 'Corrected edit' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save action' }));
		await waitFor(() => expect(mocks.updateAction).toHaveBeenCalledTimes(2));
		expect(mocks.updateAction.mock.calls[1][2].title).toBe('Corrected edit');
		expect(mocks.updateAction.mock.calls[1][2].request_id).not.toBe(
			mocks.updateAction.mock.calls[0][2].request_id
		);
	});

	it('does not allow a second create while the first result is ambiguous', async () => {
		let finishOld: (value: MeetingAction) => void = () => {};
		mocks.createAction.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					finishOld = resolve;
				})
		);
		const view = render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Create action' }));
		await fireEvent.input(screen.getByLabelText('Action title'), {
			target: { value: 'Older request' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save new action' }));
		await waitFor(() => expect(mocks.createAction).toHaveBeenCalledTimes(1));
		await view.rerender({ meetingId: 'meeting-2' });
		await view.rerender({ meetingId: 'meeting-1' });
		await fireEvent.click(screen.getByRole('button', { name: 'Create action' }));
		expect(screen.queryByRole('button', { name: 'Start another action' })).toBeNull();
		const original = mocks.createAction.mock.calls[0][1];
		await fireEvent.click(screen.getByRole('button', { name: 'Retry creation' }));
		await waitFor(() => expect(mocks.createAction).toHaveBeenCalledTimes(2));
		expect(mocks.createAction.mock.calls[1]).toEqual(['meeting-1', original]);
		await waitFor(() =>
			expect(sessionStorage.getItem('meeting-action-request:tenant-1:user-1:meeting-1')).toBeNull()
		);
		await act(() => finishOld(action()));
		expect(sessionStorage.getItem('meeting-action-request:tenant-1:user-1:meeting-1')).toBeNull();
		expect(mocks.createAction.mock.calls[0]).toEqual(mocks.createAction.mock.calls[1]);
	});

	it('reloads action choices on each edit and hides unavailable Decision metadata', async () => {
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await fireEvent.click(await screen.findByRole('button', { name: 'Edit Publish minutes' }));
		await screen.findByRole('option', { name: 'Approve publication' });
		await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
		mocks.listActions.mockResolvedValue(
			page([action({ related_decision_available: false, related_decision: null })])
		);
		mocks.listDecisions.mockResolvedValue({
			items: [{ id: 'link-1', available: false }],
			total: 1,
			page: 1,
			per_page: 50
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Refresh actions' }));
		await waitFor(() =>
			expect(screen.queryByRole('link', { name: 'Approve publication' })).toBeNull()
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Edit Publish minutes' }));
		await waitFor(() => expect(mocks.listDecisions).toHaveBeenCalledTimes(2));
		expect(screen.queryByRole('option', { name: 'Approve publication' })).toBeNull();
	});

	it('clears old action details when refresh authorization fails', async () => {
		render(MeetingActions, { props: { meetingId: 'meeting-1' } });
		await screen.findByRole('heading', { name: 'Publish minutes' });
		mocks.listActions.mockRejectedValueOnce(new ApiError(404, 'Not found'));
		await fireEvent.click(screen.getByRole('button', { name: 'Refresh actions' }));
		await screen.findByText('Could not load meeting actions.');
		expect(screen.queryByText('Publish minutes')).toBeNull();
		expect(screen.queryByText('Review with the team.')).toBeNull();
	});
});
