import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import { mailBodyText } from '$lib/mail/compose';
import MailBodyEditor from './MailBodyEditor.svelte';

describe('MailBodyEditor', () => {
	it('renders the formatting toolbar with labeled buttons', () => {
		render(MailBodyEditor, { content: '', onChange: () => {} });
		expect(screen.getByRole('toolbar', { name: 'Formatting' })).toBeTruthy();
		for (const label of [
			'Bold',
			'Italic',
			'Underline',
			'Strikethrough',
			'Inline code',
			'Add or edit link',
			'Bullet list',
			'Numbered list',
			'Quote',
			'Undo',
			'Redo'
		]) {
			expect(screen.getByRole('button', { name: label })).toBeTruthy();
		}
	});

	it('renders markdown content as rich text', async () => {
		render(MailBodyEditor, { content: 'Hello **world**', onChange: () => {} });
		const bold = await screen.findByText('world');
		expect(bold.tagName).toBe('STRONG');
	});

	it('emits markdown when the content prop is replaced externally', async () => {
		const { rerender } = render(MailBodyEditor, { content: 'First', onChange: () => {} });
		expect(await screen.findByText('First')).toBeTruthy();
		await rerender({ content: 'Second draft', onChange: () => {} });
		expect(await screen.findByText('Second draft')).toBeTruthy();
		expect(screen.queryByText('First')).toBeNull();
	});

	it('renders mail markup as text rather than loading a remote image', () => {
		const body = mailBodyText({
			type: 'html',
			content: '&lt;img src="https://tracker.example/pixel" alt="tracker"&gt;'
		});
		const { container } = render(MailBodyEditor, { content: body, onChange: () => {} });

		expect(container.querySelector('img')).toBeNull();
		expect(container.querySelector('.tiptap')?.textContent).toBe(
			'<img src="https://tracker.example/pixel" alt="tracker">'
		);

		const plainText = mailBodyText({
			type: 'text',
			content: '<img src="https://tracker.example/plain-pixel" alt="literal">'
		});
		const plainTextEditor = render(MailBodyEditor, { content: plainText, onChange: () => {} });
		expect(plainTextEditor.container.querySelector('img')).toBeNull();
		expect(plainTextEditor.container.querySelector('.tiptap')?.textContent).toBe(
			'<img src="https://tracker.example/plain-pixel" alt="literal">'
		);

		const encodedLiteral = mailBodyText({ type: 'html', content: '<p>&amp;amp;</p>' });
		const encodedLiteralEditor = render(MailBodyEditor, {
			content: encodedLiteral,
			onChange: () => {}
		});
		expect(encodedLiteralEditor.container.querySelector('.tiptap')?.textContent).toBe('&amp;');
	});
});
