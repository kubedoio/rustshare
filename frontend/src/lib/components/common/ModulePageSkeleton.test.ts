import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import ModulePageSkeleton from './ModulePageSkeleton.svelte';

describe('ModulePageSkeleton', () => {
	it('renders with aria-busy and aria-label', () => {
		const { container } = render(ModulePageSkeleton);

		const root = container.firstElementChild;
		expect(root?.getAttribute('aria-busy')).toBe('true');
		expect(root?.getAttribute('aria-label')).toBe('Loading module page');
	});

	it('renders header, toolbar and content skeleton placeholders', () => {
		const { container } = render(ModulePageSkeleton);

		// Should have animate-pulse elements for skeleton effect
		const pulseElements = container.querySelectorAll('.animate-pulse');
		expect(pulseElements.length).toBeGreaterThan(0);
	});
});
