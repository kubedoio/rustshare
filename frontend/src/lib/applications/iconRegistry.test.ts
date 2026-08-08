import { describe, expect, it } from 'vitest';
import {
	APPROVED_MODULE_ICONS,
	DEFAULT_MODULE_ICON,
	isApprovedApplicationIcon
} from './iconRegistry';

describe('icon registry', () => {
	it('exports the approved icon keys used by modules and templates', () => {
		expect(APPROVED_MODULE_ICONS).toContain('sticky-note');
		expect(APPROVED_MODULE_ICONS).toContain('calendar-days');
		expect(APPROVED_MODULE_ICONS).toContain('clipboard-list');
		expect(APPROVED_MODULE_ICONS).toContain('share-2');
		expect(APPROVED_MODULE_ICONS).toContain('lightbulb');
		expect(APPROVED_MODULE_ICONS).toContain('activity');
		expect(APPROVED_MODULE_ICONS).toContain('path-separation');
		expect(DEFAULT_MODULE_ICON).toBe('folder');
	});

	it('recognizes approved icons and rejects unknown ones', () => {
		expect(isApprovedApplicationIcon('columns')).toBe(true);
		expect(isApprovedApplicationIcon('users')).toBe(false);
	});
});
