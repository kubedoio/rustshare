import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AskExperience from './AskExperience.svelte';
import { ApiError } from '$lib/api/types';

const mocks = vi.hoisted(() => ({ askWorkspace: vi.fn(), openAskCitation: vi.fn() }));

vi.mock('$lib/api/ask', () => ({
	askWorkspace: mocks.askWorkspace,
	openAskCitation: mocks.openAskCitation
}));
vi.mock('$lib/stores/auth', () => ({ currentUser: readable({ tenant_id: 'tenant-1' }) }));

describe('AskExperience', () => {
	beforeEach(() => {
		mocks.askWorkspace.mockReset();
		mocks.openAskCitation.mockReset();
		mocks.openAskCitation.mockResolvedValue({
			available: true,
			resource_ref: 'elembra://io.elembra.chat/message/m-1'
		});
	});

	it('uses one component for a scoped question and renders returned citations', async () => {
		mocks.askWorkspace.mockResolvedValue({
			answer: 'The answer',
			citations: [
				{
					resource_ref: 'elembra://io.elembra.files/file/f-1',
					title: 'Plan.md',
					location: '/Plan.md',
					provenance: {}
				}
			],
			source_count: 1,
			grounded: true,
			insufficient_evidence: false,
			run_id: 'run-1'
		});
		const onChatCitationOpen = vi.fn();
		const { container } = render(AskExperience, {
			props: {
				scope: { type: 'note', resourceRef: 'elembra://io.elembra.files/file/f-1' },
				scopeLabel: 'Current note',
				onChatCitationOpen
			}
		});

		await fireEvent.input(screen.getByLabelText('Your question'), {
			target: { value: 'What is this?' }
		});
		await fireEvent.submit(container.querySelector('form')!);
		await waitFor(() => expect(screen.getByText('The answer')).toBeTruthy());
		expect(mocks.askWorkspace).toHaveBeenCalledWith(
			expect.objectContaining({
				workspace_id: 'tenant-1',
				scope: { type: 'note', resourceRef: 'elembra://io.elembra.files/file/f-1' }
			})
		);
		expect(screen.getByText('Plan.md')).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: /Plan\.md/ }));
		expect(mocks.openAskCitation).toHaveBeenCalledWith('elembra://io.elembra.files/file/f-1');
		expect(onChatCitationOpen).toHaveBeenCalledWith({
			available: true,
			resource_ref: 'elembra://io.elembra.chat/message/m-1'
		});
	});

	it('renders provider failure without exposing source details', async () => {
		mocks.askWorkspace.mockRejectedValue(new ApiError(503, 'unavailable'));
		render(AskExperience);
		await fireEvent.input(screen.getByLabelText('Your question'), {
			target: { value: 'Question' }
		});
		await fireEvent.submit(screen.getByRole('button', { name: 'Ask' }).closest('form')!);
		await waitFor(() => expect(screen.getByText(/temporarily unavailable/)).toBeTruthy());
	});

	it('renders rate limits and insufficient evidence as distinct states', async () => {
		mocks.askWorkspace.mockRejectedValueOnce(new ApiError(429, 'busy'));
		render(AskExperience);
		await fireEvent.input(screen.getByLabelText('Your question'), {
			target: { value: 'Question' }
		});
		await fireEvent.submit(screen.getByRole('button', { name: 'Ask' }).closest('form')!);
		await waitFor(() => expect(screen.getByText(/busy right now/)).toBeTruthy());

		mocks.askWorkspace.mockResolvedValueOnce({
			answer: 'There is not enough authorized evidence to answer this question.',
			citations: [],
			source_count: 0,
			grounded: false,
			insufficient_evidence: true,
			run_id: 'run-insufficient'
		});
		await fireEvent.input(screen.getByLabelText('Your question'), {
			target: { value: 'Another question' }
		});
		await fireEvent.submit(screen.getByRole('button', { name: 'Ask' }).closest('form')!);
		await waitFor(() => expect(screen.getByText('Insufficient evidence')).toBeTruthy());
	});

	it('hides unavailable citation details behind a generic reauthorization result', async () => {
		mocks.askWorkspace.mockResolvedValue({
			answer: 'Grounded answer',
			citations: [
				{
					resource_ref: 'elembra://io.elembra.files/file/f-denied',
					title: 'Visible before revocation',
					location: '/private/marker.txt',
					provenance: { file_id: 'f-denied' }
				}
			],
			source_count: 1,
			grounded: true,
			insufficient_evidence: false,
			run_id: 'run-revoked'
		});
		mocks.openAskCitation.mockResolvedValue({
			available: false,
			resource_ref: 'elembra://io.elembra.files/file/f-denied'
		});
		const { container } = render(AskExperience);
		await fireEvent.input(screen.getByLabelText('Your question'), {
			target: { value: 'Question' }
		});
		await fireEvent.submit(container.querySelector('form')!);
		await waitFor(() => expect(screen.getByText('Visible before revocation')).toBeTruthy());
		await fireEvent.click(screen.getByRole('button', { name: /Visible before revocation/ }));
		await waitFor(() =>
			expect(screen.getByText('That source is no longer available.')).toBeTruthy()
		);
		expect(screen.queryByText('unauthorized marker')).toBeNull();
	});

	it('does not let an older response overwrite a newer question', async () => {
		let resolveFirst!: (value: unknown) => void;
		const first = new Promise((resolve) => (resolveFirst = resolve));
		mocks.askWorkspace.mockReturnValueOnce(first);
		const view = render(AskExperience);
		const input = screen.getByLabelText('Your question');
		const form = screen.getByRole('button', { name: 'Ask' }).closest('form')!;
		await fireEvent.input(input, { target: { value: 'First' } });
		await fireEvent.submit(form);
		await view.rerender({
			scope: { type: 'folder', resourceRef: 'elembra://io.elembra.files/folder/f-2' },
			scopeLabel: 'Folder'
		});
		resolveFirst({
			answer: 'First answer',
			citations: [],
			source_count: 0,
			grounded: false,
			insufficient_evidence: true,
			run_id: '1'
		});
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(screen.queryByText('First answer')).toBeNull();
	});
});
