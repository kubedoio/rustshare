import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import SortableTableHeader from './SortableTableHeader.svelte';

describe('SortableTableHeader', () => {
	it('should render label', () => {
		const { getByText } = render(SortableTableHeader, {
			props: {
				label: 'Name',
				field: 'name',
				activeField: 'modified_at',
				activeOrder: 'asc',
				onSort: vi.fn()
			}
		});
		expect(getByText('Name')).toBeTruthy();
	});

	it('should call onSort when button is clicked', async () => {
		const onSort = vi.fn();
		const { getByRole } = render(SortableTableHeader, {
			props: {
				label: 'Size',
				field: 'size',
				activeField: 'name',
				activeOrder: 'asc',
				onSort
			}
		});
		await fireEvent.click(getByRole('button'));
		expect(onSort).toHaveBeenCalledWith('size');
	});

	it('should call onSort when button is activated with Enter', async () => {
		const onSort = vi.fn();
		const { getByRole } = render(SortableTableHeader, {
			props: {
				label: 'Size',
				field: 'size',
				activeField: 'name',
				activeOrder: 'asc',
				onSort
			}
		});
		const button = getByRole('button');
		await fireEvent.keyDown(button, { key: 'Enter' });
		await fireEvent.click(button);
		expect(onSort).toHaveBeenCalledWith('size');
	});

	it('should call onSort when button is activated with Space', async () => {
		const onSort = vi.fn();
		const { getByRole } = render(SortableTableHeader, {
			props: {
				label: 'Size',
				field: 'size',
				activeField: 'name',
				activeOrder: 'asc',
				onSort
			}
		});
		const button = getByRole('button');
		await fireEvent.keyDown(button, { key: ' ' });
		await fireEvent.click(button);
		expect(onSort).toHaveBeenCalledWith('size');
	});

	it('should have aria-sort="descending" when active descending', () => {
		const { getByRole } = render(SortableTableHeader, {
			props: {
				label: 'Name',
				field: 'name',
				activeField: 'name',
				activeOrder: 'desc',
				onSort: vi.fn()
			}
		});
		expect(getByRole('columnheader').getAttribute('aria-sort')).toBe('descending');
	});

	it('should have aria-sort="ascending" when active ascending', () => {
		const { getByRole } = render(SortableTableHeader, {
			props: {
				label: 'Name',
				field: 'name',
				activeField: 'name',
				activeOrder: 'asc',
				onSort: vi.fn()
			}
		});
		expect(getByRole('columnheader').getAttribute('aria-sort')).toBe('ascending');
	});

	it('should have aria-sort="none" when inactive', () => {
		const { getByRole } = render(SortableTableHeader, {
			props: {
				label: 'Name',
				field: 'name',
				activeField: 'size',
				activeOrder: 'asc',
				onSort: vi.fn()
			}
		});
		expect(getByRole('columnheader').getAttribute('aria-sort')).toBe('none');
	});

	it('should apply custom class prop to th element', () => {
		const { getByRole } = render(SortableTableHeader, {
			props: {
				label: 'Name',
				field: 'name',
				activeField: 'size',
				activeOrder: 'asc',
				onSort: vi.fn(),
				class: 'custom-header-class'
			}
		});
		expect(getByRole('columnheader').classList.contains('custom-header-class')).toBe(true);
	});

	it('should render ArrowUp icon when active ascending', () => {
		const { container } = render(SortableTableHeader, {
			props: {
				label: 'Name',
				field: 'name',
				activeField: 'name',
				activeOrder: 'asc',
				onSort: vi.fn()
			}
		});
		const svg = container.querySelector('svg');
		expect(svg).toBeTruthy();
		// lucide-svelte ArrowUp icon
		expect(svg?.getAttribute('xmlns')).toBe('http://www.w3.org/2000/svg');
		expect(svg?.getAttribute('viewBox')).toBe('0 0 24 24');
		expect(svg?.getAttribute('fill')).toBe('none');
		expect(svg?.getAttribute('stroke')).toBe('currentColor');
		const paths = Array.from(svg?.querySelectorAll('path') ?? []);
		const ds = paths.map((p) => p.getAttribute('d'));
		expect(ds).toContain('m5 12 7-7 7 7');
		expect(ds).toContain('M12 19V5');
	});

	it('should render ArrowDown icon when active descending', () => {
		const { container } = render(SortableTableHeader, {
			props: {
				label: 'Name',
				field: 'name',
				activeField: 'name',
				activeOrder: 'desc',
				onSort: vi.fn()
			}
		});
		const svg = container.querySelector('svg');
		expect(svg).toBeTruthy();
		// lucide-svelte ArrowDown icon
		expect(svg?.getAttribute('xmlns')).toBe('http://www.w3.org/2000/svg');
		expect(svg?.getAttribute('viewBox')).toBe('0 0 24 24');
		expect(svg?.getAttribute('fill')).toBe('none');
		expect(svg?.getAttribute('stroke')).toBe('currentColor');
		const paths = Array.from(svg?.querySelectorAll('path') ?? []);
		const ds = paths.map((p) => p.getAttribute('d'));
		expect(ds).toContain('M12 5v14');
		expect(ds).toContain('m19 12-7 7-7-7');
	});

	it('should render ArrowUpDown icon when inactive', () => {
		const { container } = render(SortableTableHeader, {
			props: {
				label: 'Name',
				field: 'name',
				activeField: 'size',
				activeOrder: 'asc',
				onSort: vi.fn()
			}
		});
		const svg = container.querySelector('svg');
		expect(svg).toBeTruthy();
		// lucide-svelte ArrowUpDown icon
		expect(svg?.getAttribute('xmlns')).toBe('http://www.w3.org/2000/svg');
		expect(svg?.getAttribute('viewBox')).toBe('0 0 24 24');
		expect(svg?.getAttribute('fill')).toBe('none');
		expect(svg?.getAttribute('stroke')).toBe('currentColor');
		const paths = Array.from(svg?.querySelectorAll('path') ?? []);
		const ds = paths.map((p) => p.getAttribute('d'));
		expect(ds).toContain('m21 16-4 4-4-4');
		expect(ds).toContain('M17 20V4');
		expect(ds).toContain('m3 8 4-4 4 4');
		expect(ds).toContain('M7 4v16');
	});
});
