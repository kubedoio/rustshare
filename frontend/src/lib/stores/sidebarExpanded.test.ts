import { describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import { sidebarExpanded } from './sidebarExpanded';

describe('sidebarExpanded store', () => {
	it('defaults to false', () => {
		expect(get(sidebarExpanded)).toBe(false);
	});

	it('can be toggled', () => {
		sidebarExpanded.toggle();
		expect(get(sidebarExpanded)).toBe(true);

		sidebarExpanded.toggle();
		expect(get(sidebarExpanded)).toBe(false);
	});

	it('can be explicitly set', () => {
		sidebarExpanded.set(true);
		expect(get(sidebarExpanded)).toBe(true);

		sidebarExpanded.set(false);
		expect(get(sidebarExpanded)).toBe(false);
	});
});
