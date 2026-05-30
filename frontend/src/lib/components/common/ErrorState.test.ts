import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import ErrorState from './ErrorState.svelte';

describe('ErrorState', () => {
	it('renders default title and message', () => {
		render(ErrorState);

		expect(screen.getByText('Something went wrong')).toBeTruthy();
		expect(
			screen.getByText('Failed to load data. Please check your connection and try again.')
		).toBeTruthy();
	});

	it('renders custom title and message', () => {
		render(ErrorState, {
			props: {
				title: 'Custom Error',
				message: 'Custom message here'
			}
		});

		expect(screen.getByText('Custom Error')).toBeTruthy();
		expect(screen.getByText('Custom message here')).toBeTruthy();
	});

	it('renders retry button when onRetry is provided', () => {
		const onRetry = vi.fn();
		render(ErrorState, {
			props: { onRetry }
		});

		const button = screen.getByRole('button', { name: /try again/i });
		expect(button).toBeTruthy();
	});

	it('does not render retry button when onRetry is omitted', () => {
		render(ErrorState);

		expect(screen.queryByRole('button')).toBeNull();
	});

	it('calls onRetry when retry button is clicked', async () => {
		const onRetry = vi.fn();
		render(ErrorState, {
			props: { onRetry }
		});

		const button = screen.getByRole('button', { name: /try again/i });
		await fireEvent.click(button);

		expect(onRetry).toHaveBeenCalledTimes(1);
	});

	it('uses custom retry label when provided', () => {
		render(ErrorState, {
			props: {
				onRetry: vi.fn(),
				retryLabel: 'Reload'
			}
		});

		expect(screen.getByRole('button', { name: 'Reload' })).toBeTruthy();
	});
});
