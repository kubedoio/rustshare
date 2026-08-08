import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
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
});
