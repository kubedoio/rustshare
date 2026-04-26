import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import PaginationControls from './PaginationControls.svelte';

describe('PaginationControls', () => {
  let onPageChange: ReturnType<typeof vi.fn>;
  let onPageSizeChange: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    onPageChange = vi.fn();
    onPageSizeChange = vi.fn();
  });

  function getProps(overrides: Partial<{ currentPage: number; totalPages: number; pageSize: 10 | 20 | 50; onPageChange: any; onPageSizeChange: any }> = {}) {
    return {
      currentPage: 1,
      totalPages: 5,
      pageSize: 10 as const,
      onPageChange: onPageChange as any,
      onPageSizeChange: onPageSizeChange as any,
      ...overrides
    };
  }

  it('renders page buttons for all pages', () => {
    render(PaginationControls, { props: getProps() });

    for (let i = 1; i <= 5; i++) {
      expect(screen.getByLabelText(`Page ${i}`)).toBeTruthy();
    }
  });

  it('truncates page buttons when there are many pages', () => {
    render(PaginationControls, {
      props: getProps({ currentPage: 10, totalPages: 20 })
    });

    // Should show first page, ellipsis, current window, ellipsis, last page
    expect(screen.getByLabelText('Page 1')).toBeTruthy();
    expect(screen.getByLabelText('Page 9')).toBeTruthy();
    expect(screen.getByLabelText('Page 10')).toBeTruthy();
    expect(screen.getByLabelText('Page 11')).toBeTruthy();
    expect(screen.getByLabelText('Page 20')).toBeTruthy();

    // Pages outside the window should not be rendered
    expect(screen.queryByLabelText('Page 5')).toBeNull();
    expect(screen.queryByLabelText('Page 15')).toBeNull();

    // Should have ellipsis separators (aria-hidden spans)
    const ellipsis = screen.getAllByText('...');
    expect(ellipsis.length).toBe(2);
  });

  it('shows all pages without ellipsis when total pages is 7 or fewer', () => {
    render(PaginationControls, {
      props: getProps({ currentPage: 4, totalPages: 7 })
    });

    for (let i = 1; i <= 7; i++) {
      expect(screen.getByLabelText(`Page ${i}`)).toBeTruthy();
    }

    expect(screen.queryByText('...')).toBeNull();
  });

  it('calls onPageChange with correct page when a page button is clicked', () => {
    render(PaginationControls, { props: getProps() });

    const page3 = screen.getByLabelText('Page 3');
    fireEvent.click(page3);

    expect(onPageChange).toHaveBeenCalledTimes(1);
    expect(onPageChange).toHaveBeenCalledWith(3);
  });

  it('calls onPageChange with previous page when Previous is clicked', () => {
    render(PaginationControls, {
      props: getProps({ currentPage: 3 })
    });

    const previous = screen.getByLabelText('Previous page');
    fireEvent.click(previous);

    expect(onPageChange).toHaveBeenCalledTimes(1);
    expect(onPageChange).toHaveBeenCalledWith(2);
  });

  it('calls onPageChange with next page when Next is clicked', () => {
    render(PaginationControls, {
      props: getProps({ currentPage: 3 })
    });

    const next = screen.getByLabelText('Next page');
    fireEvent.click(next);

    expect(onPageChange).toHaveBeenCalledTimes(1);
    expect(onPageChange).toHaveBeenCalledWith(4);
  });

  it('disables Previous on page 1', () => {
    render(PaginationControls, { props: getProps() });

    const previous = screen.getByLabelText('Previous page') as HTMLButtonElement;
    expect(previous.disabled).toBe(true);
  });

  it('disables Next on last page', () => {
    render(PaginationControls, {
      props: getProps({ currentPage: 5 })
    });

    const next = screen.getByLabelText('Next page') as HTMLButtonElement;
    expect(next.disabled).toBe(true);
  });

  it('highlights current page visually with aria-current="page"', () => {
    render(PaginationControls, {
      props: getProps({ currentPage: 2 })
    });

    const page2 = screen.getByLabelText('Page 2');
    expect(page2.getAttribute('aria-current')).toBe('page');

    const page1 = screen.getByLabelText('Page 1');
    expect(page1.getAttribute('aria-current')).toBeNull();
  });

  it('calls onPageSizeChange when select value changes', () => {
    render(PaginationControls, { props: getProps() });

    const select = screen.getByLabelText('Items per page');
    fireEvent.change(select, { target: { value: '50' } });

    expect(onPageSizeChange).toHaveBeenCalledTimes(1);
    expect(onPageSizeChange).toHaveBeenCalledWith(50);
  });
});
