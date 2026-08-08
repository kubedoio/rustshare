import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import ApplicationPageSkeleton from './ApplicationPageSkeleton.svelte';

describe('ApplicationPageSkeleton', () => {
	it('renders with aria-busy and aria-label', () => {
		const { container } = render(ApplicationPageSkeleton);

		const root = container.firstElementChild;
		expect(root?.getAttribute('aria-busy')).toBe('true');
		expect(root?.getAttribute('aria-label')).toBe('Loading module page');
	});

	it('renders header, toolbar and content skeleton placeholders', () => {
		const { container } = render(ApplicationPageSkeleton);

		// Should have animate-pulse elements for skeleton effect
		const pulseElements = container.querySelectorAll('.animate-pulse');
		expect(pulseElements.length).toBeGreaterThan(0);
	});
});
