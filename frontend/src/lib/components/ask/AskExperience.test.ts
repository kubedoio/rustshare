import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AskExperience from './AskExperience.svelte';
import { ApiError } from '$lib/api/types';

const mocks = vi.hoisted(() => ({
	askWorkspace: vi.fn(),
	openAskCitation: vi.fn(),
	searchMemory: vi.fn(),
	goto: vi.fn()
}));

vi.mock('$lib/api/ask', () => ({
	askWorkspace: mocks.askWorkspace,
	openAskCitation: mocks.openAskCitation,
	searchMemory: mocks.searchMemory
}));
vi.mock('$lib/stores/auth', () => ({ currentUser: readable({ tenant_id: 'tenant-1' }) }));
vi.mock('$app/navigation', () => ({ goto: mocks.goto }));

describe('AskExperience', () => {
	beforeEach(() => {
		mocks.askWorkspace.mockReset();
		mocks.openAskCitation.mockReset();
		mocks.searchMemory.mockReset();
		mocks.goto.mockReset();
		mocks.openAskCitation.mockResolvedValue({
			available: true,
			resource_ref: 'elembra://io.elembra.chat/message/m-1'
		});
	});

	it('offers keyword search for workspace Memory without an LLM', async () => {
		mocks.openAskCitation.mockResolvedValue({
			available: true,
			resource_ref: 'elembra://io.elembra.files/file/f-1'
		});
		mocks.searchMemory.mockResolvedValue({
			total: 1,
			results: [
				{
					source_application: 'io.elembra.files',
					source_type: 'note',
					resource_ref: 'elembra://io.elembra.files/file/f-1',
					title: 'Runbook.md',
					snippet: 'restart the service',
					location: '/ops/Runbook.md',
					provenance: {}
				}
			]
		});
		render(AskExperience);
		await fireEvent.input(screen.getByLabelText('Your question'), {
			target: { value: 'restart service' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Search by keyword' }));
		await waitFor(() => expect(screen.getByText('Runbook.md')).toBeTruthy());
		expect(mocks.searchMemory).toHaveBeenCalledWith('restart service');
		expect(screen.getByText('restart the service')).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: /Runbook\.md/ }));
		expect(mocks.openAskCitation).toHaveBeenCalledWith('elembra://io.elembra.files/file/f-1');
		expect(mocks.goto).toHaveBeenCalledWith('/files?preview=f-1');
	});

	it('ignores keyword search results from a previous scope after switching back', async () => {
		let resolveFirst!: (value: { total: number; results: unknown[] }) => void;
		let resolveSecond!: (value: { total: number; results: unknown[] }) => void;
		mocks.searchMemory
			.mockReturnValueOnce(new Promise((resolve) => (resolveFirst = resolve)))
			.mockReturnValueOnce(new Promise((resolve) => (resolveSecond = resolve)));
		const view = render(AskExperience);
		await fireEvent.input(screen.getByLabelText('Your question'), { target: { value: 'term' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Search by keyword' }));

		await view.rerender({
			scope: { type: 'folder', resourceRef: 'elembra://io.elembra.files/folder/f-2' },
			scopeLabel: 'Folder'
		});
		await view.rerender({ scope: { type: 'workspace' }, scopeLabel: 'Workspace' });
		const searchButton = screen.getByRole('button', { name: 'Search by keyword' });
		expect((searchButton as HTMLButtonElement).disabled).toBe(false);
		await fireEvent.click(searchButton);
		resolveSecond({
			total: 1,
			results: [
				{
					source_application: 'io.elembra.files',
					source_type: 'note',
					resource_ref: 'elembra://io.elembra.files/file/current',
					title: 'Current result',
					provenance: {}
				}
			]
		});
		await waitFor(() => expect(screen.getByText('Current result')).toBeTruthy());

		resolveFirst({
			total: 1,
			results: [
				{
					source_application: 'io.elembra.files',
					source_type: 'note',
					resource_ref: 'elembra://io.elembra.files/file/stale',
					title: 'Stale result',
					provenance: {}
				}
			]
		});
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(screen.queryByText('Stale result')).toBeNull();
		expect(screen.getByText('Current result')).toBeTruthy();
	});

	it('keeps Ask available when a keyword query exceeds the search limit', async () => {
		render(AskExperience);
		await fireEvent.input(screen.getByLabelText('Your question'), {
			target: { value: 'a'.repeat(1_001) }
		});
		expect(screen.getByRole('button', { name: 'Search by keyword' }).hasAttribute('disabled')).toBe(
			true
		);
		expect(screen.getByText(/Keyword search supports up to 1000 characters/)).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Ask' }).hasAttribute('disabled')).toBe(false);
		expect(mocks.searchMemory).not.toHaveBeenCalled();
	});

	it('opens a cited Note at the exact authorized historical version', async () => {
		mocks.askWorkspace.mockResolvedValue({
			answer: 'Grounded answer',
			citations: [
				{
					resource_ref:
						'elembra://io.elembra.files/file/n-1?version=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
					title: 'Plan.md',
					location: '/Plan.md',
					provenance: { note_id: 'n-1' }
				}
			],
			source_count: 1,
			grounded: true,
			insufficient_evidence: false,
			run_id: 'run-1'
		});
		mocks.openAskCitation.mockResolvedValue({
			available: true,
			resource_ref:
				'elembra://io.elembra.files/file/n-1?version=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
		});
		const { container } = render(AskExperience);
		await fireEvent.input(screen.getByLabelText('Your question'), {
			target: { value: 'What changed?' }
		});
		await fireEvent.submit(container.querySelector('form')!);
		await waitFor(() => expect(screen.getByText('Plan.md')).toBeTruthy());
		await fireEvent.click(screen.getByRole('button', { name: /Plan\.md/ }));
		expect(mocks.goto).toHaveBeenCalledWith(
			'/apps/notes/n-1?sourceVersion=sha256%3Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
		);
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

	it('recovers from a scope switch while a question is in flight', async () => {
		let resolveFirst!: (value: unknown) => void;
		mocks.askWorkspace.mockReturnValueOnce(new Promise((resolve) => (resolveFirst = resolve)));
		const view = render(AskExperience);
		const input = screen.getByLabelText('Your question');
		const form = screen.getByRole('button', { name: 'Ask' }).closest('form')!;
		await fireEvent.input(input, { target: { value: 'First' } });
		await fireEvent.submit(form);
		expect((screen.getByRole('button', { name: 'Generating' }) as HTMLButtonElement).disabled).toBe(
			true
		);

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

		// The stale request's finally must not leave the form stuck disabled.
		const askButton = screen.getByRole('button', { name: 'Ask' }) as HTMLButtonElement;
		expect(askButton.disabled).toBe(false);
	});
});
