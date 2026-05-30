import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import FileGridSkeleton from './FileGridSkeleton.svelte';

describe('FileGridSkeleton', () => {
	it('renders with aria-busy and aria-label', () => {
		const { container } = render(FileGridSkeleton);

		const root = container.firstElementChild;
		expect(root?.getAttribute('aria-busy')).toBe('true');
		expect(root?.getAttribute('aria-label')).toBe('Loading file grid');
	});

	it('renders 12 grid item placeholders', () => {
		const { container } = render(FileGridSkeleton);

		// Each of 12 items contains 3 animate-pulse sub-elements
		const pulseElements = container.querySelectorAll('.animate-pulse');
		expect(pulseElements.length).toBe(36);
	});
});
