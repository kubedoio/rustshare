import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import MailComposeModal from './MailComposeModal.svelte';

const mocks = vi.hoisted(() => ({
	listAllFiles: vi.fn()
}));

vi.mock('$lib/api/files', () => ({
	listAllFiles: mocks.listAllFiles
}));

describe('MailComposeModal', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.listAllFiles.mockResolvedValue([]);
	});

	it('saves a new draft and sends the message payload', async () => {
		const onSave = vi.fn();
		const onSend = vi.fn();
		render(MailComposeModal, {
			open: true,
			hasSmtp: true,
			onClose: vi.fn(),
			onSave,
			onSend
		});

		await fireEvent.input(screen.getByPlaceholderText('To'), {
			target: { value: 'bob@example.com' }
		});
		await fireEvent.input(screen.getByPlaceholderText('Subject'), {
			target: { value: 'Hello' }
		});
		await fireEvent.input(screen.getByPlaceholderText('Message'), {
			target: { value: 'Hi Bob' }
		});

		await fireEvent.click(screen.getByRole('button', { name: /save draft/i }));
		expect(onSave).toHaveBeenCalledWith(
			{
				to: ['bob@example.com'],
				cc: [],
				bcc: [],
				subject: 'Hello',
				body: 'Hi Bob',
				attachments: [],
				in_reply_to_msg_id: null
			},
			null
		);

		await fireEvent.submit(screen.getByPlaceholderText('Message').closest('form')!);
		expect(onSend).toHaveBeenCalledWith(
			expect.objectContaining({
				to: ['bob@example.com'],
				cc: [],
				bcc: [],
				subject: 'Hello',
				body: 'Hi Bob',
				attachments: [],
				in_reply_to_msg_id: null,
				idempotency_key: expect.stringMatching(/^[0-9a-f-]{36}$/)
			})
		);
	});

	it('shows draft-edit actions for an existing draft', async () => {
		const onDiscard = vi.fn();
		render(MailComposeModal, {
			open: true,
			hasSmtp: true,
			mode: 'draft-edit',
			draftId: 'draft-1',
			initialTo: 'alice@example.com',
			initialSubject: 'Draft subject',
			initialBody: 'Draft body',
			onClose: vi.fn(),
			onSave: vi.fn(),
			onSend: vi.fn(),
			onDiscard
		});

		expect(await screen.findByRole('button', { name: /discard/i })).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: /save changes/i }));
		expect(screen.getByText('Saved draft')).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: /discard/i }));
		await waitFor(() => {
			expect(onDiscard).toHaveBeenCalledWith('draft-1');
		});
	});
});
