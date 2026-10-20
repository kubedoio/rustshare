import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import DecisionMeetingContext from './DecisionMeetingContext.svelte';

const mocks = vi.hoisted(() => ({ listMeetings: vi.fn(), update: vi.fn(), toast: vi.fn() }));
vi.mock('$lib/api/decisions', () => ({ decisionsApi: mocks }));
vi.mock('$lib/stores/toast', () => ({ toastStore: { show: mocks.toast } }));

describe('DecisionMeetingContext', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.listMeetings.mockResolvedValue({
			items: [
				{ id: 'meeting-1', title: 'Budget review', calendar_event_id: 'event-1', is_origin: true }
			]
		});
		mocks.update.mockResolvedValue({});
	});

	it('navigates back to the independently authorized origin meeting and event', async () => {
		render(DecisionMeetingContext, { props: { decisionId: 'decision-1', status: 'proposed' } });
		expect(
			(await screen.findByRole('link', { name: 'Origin meeting: Budget review' })).getAttribute(
				'href'
			)
		).toBe('/apps/meetings/meeting-1');
		expect(screen.getByRole('link', { name: 'Calendar event' }).getAttribute('href')).toBe(
			'/apps/calendar?event_id=event-1'
		);
	});

	it('loads authorized meeting history on demand and avoids duplicate contexts', async () => {
		const firstMeeting = { id: 'meeting-1', title: 'Budget review', is_origin: true };
		mocks.listMeetings
			.mockResolvedValueOnce({ items: [firstMeeting], next_page: 2 })
			.mockResolvedValueOnce({
				items: [
					firstMeeting,
					{ id: 'meeting-2', title: 'Accessible historical meeting', is_origin: false }
				],
				next_page: null
			});
		render(DecisionMeetingContext, { props: { decisionId: 'decision-1' } });
		await screen.findByRole('button', { name: 'Load more meeting context' });
		expect(mocks.listMeetings).toHaveBeenCalledTimes(1);
		await fireEvent.click(screen.getByRole('button', { name: 'Load more meeting context' }));
		expect(
			(
				await screen.findByRole('link', { name: 'Meeting: Accessible historical meeting' })
			).getAttribute('href')
		).toBe('/apps/meetings/meeting-2');
		expect(mocks.listMeetings).toHaveBeenNthCalledWith(1, 'decision-1', 1);
		expect(mocks.listMeetings).toHaveBeenNthCalledWith(2, 'decision-1', 2);
		expect(screen.getAllByRole('link', { name: 'Origin meeting: Budget review' })).toHaveLength(1);
		expect(screen.queryByRole('button', { name: 'Load more meeting context' })).toBeNull();
	});

	it('edits status and date through Decisions rather than the meeting association', async () => {
		const onUpdated = vi.fn();
		render(DecisionMeetingContext, {
			props: { decisionId: 'decision-1', status: 'proposed', onUpdated }
		});
		await fireEvent.change(screen.getByLabelText('Decision status'), {
			target: { value: 'accepted' }
		});
		expect((screen.getByLabelText('Decision status') as HTMLSelectElement).value).toBe('accepted');
		await fireEvent.input(screen.getByLabelText('Decision date'), {
			target: { value: '2026-10-21' }
		});
		expect((screen.getByLabelText('Decision status') as HTMLSelectElement).value).toBe('accepted');
		await fireEvent.click(screen.getByRole('button', { name: 'Save decision details' }));
		await waitFor(() =>
			expect(mocks.update).toHaveBeenCalledWith('decision-1', {
				status: 'accepted',
				decision_date: '2026-10-21T00:00:00Z'
			})
		);
		expect(onUpdated).toHaveBeenCalledOnce();
	});

	it('normalizes legacy status and RFC3339 dates into editor values', async () => {
		render(DecisionMeetingContext, {
			props: {
				decisionId: 'decision-1',
				status: 'Deprecated',
				decisionDate: '2026-10-20T12:30:00Z'
			}
		});
		expect((screen.getByLabelText('Decision status') as HTMLSelectElement).value).toBe(
			'deprecated'
		);
		expect((screen.getByLabelText('Decision date') as HTMLInputElement).value).toBe('2026-10-20');
		expect(screen.getByRole('option', { name: 'Rejected' })).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: 'Save decision details' }));
		await waitFor(() =>
			expect(mocks.update).toHaveBeenCalledWith('decision-1', { status: 'deprecated' })
		);
	});

	it('offers an explicit proposed state for a legacy Decision with no status', async () => {
		render(DecisionMeetingContext, { props: { decisionId: 'legacy-decision', status: '' } });
		expect((screen.getByLabelText('Decision status') as HTMLSelectElement).value).toBe('proposed');
	});

	it('handles a null decision date and still allows status updates', async () => {
		render(DecisionMeetingContext, {
			props: { decisionId: 'decision-null-date', status: 'proposed', decisionDate: null }
		});
		expect((screen.getByLabelText('Decision date') as HTMLInputElement).value).toBe('');
		await fireEvent.change(screen.getByLabelText('Decision status'), {
			target: { value: 'accepted' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save decision details' }));
		await waitFor(() =>
			expect(mocks.update).toHaveBeenCalledWith('decision-null-date', { status: 'accepted' })
		);
	});

	it('shows a generic mutation error and retains the draft when editing is denied', async () => {
		mocks.update.mockRejectedValueOnce(new Error('secret database detail'));
		render(DecisionMeetingContext, { props: { decisionId: 'decision-1', status: 'proposed' } });
		await fireEvent.change(screen.getByLabelText('Decision status'), {
			target: { value: 'accepted' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save decision details' }));
		expect((await screen.findByRole('alert')).textContent).toContain(
			'Editing requires permission on this decision'
		);
		expect(screen.queryByText('secret database detail')).toBeNull();
		expect((screen.getByLabelText('Decision status') as HTMLSelectElement).value).toBe('accepted');
	});

	it('clears the previous decision draft and stale meeting context after navigation', async () => {
		let resolveOld: (value: unknown) => void = () => {};
		mocks.listMeetings.mockImplementationOnce(
			() =>
				new Promise((resolve) => {
					resolveOld = resolve;
				})
		);
		const view = render(DecisionMeetingContext, {
			props: { decisionId: 'decision-old', status: 'proposed' }
		});
		await fireEvent.change(screen.getByLabelText('Decision status'), {
			target: { value: 'accepted' }
		});
		mocks.listMeetings.mockResolvedValue({ items: [] });
		await view.rerender({ decisionId: 'decision-new', status: 'proposed' });
		expect((screen.getByLabelText('Decision status') as HTMLSelectElement).value).toBe('proposed');
		resolveOld({
			items: [{ id: 'private-old-meeting', title: 'Old private title', is_origin: true }]
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save decision details' }));
		await waitFor(() =>
			expect(mocks.update).toHaveBeenCalledWith('decision-new', { status: 'proposed' })
		);
		expect(screen.queryByText('Origin meeting: Old private title')).toBeNull();
	});
});
