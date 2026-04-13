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

	it('should call onSort when clicked', async () => {
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
		await fireEvent.click(getByRole('columnheader'));
		expect(onSort).toHaveBeenCalledWith('size');
	});

	it('should have aria-sort when active', () => {
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
});
