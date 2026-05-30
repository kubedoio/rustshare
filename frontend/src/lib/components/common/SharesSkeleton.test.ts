import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import SharesSkeleton from './SharesSkeleton.svelte';

describe('SharesSkeleton', () => {
	it('renders with aria-busy and aria-label', () => {
		const { container } = render(SharesSkeleton);

		const root = container.firstElementChild;
		expect(root?.getAttribute('aria-busy')).toBe('true');
		expect(root?.getAttribute('aria-label')).toBe('Loading shares');
	});

	it('renders skeleton placeholders for header, stats and list', () => {
		const { container } = render(SharesSkeleton);

		const pulseElements = container.querySelectorAll('.animate-pulse');
		expect(pulseElements.length).toBeGreaterThan(0);
	});
});
