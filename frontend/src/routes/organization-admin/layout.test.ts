import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const capability = vi.hoisted(() => ({ get: vi.fn() }));

vi.mock('$lib/api/organization-admin', () => ({
	getOrganizationAdminCapability: capability.get
}));

import OrganizationAdminLayout from './+layout.svelte';

describe('organization administration authorization shell', () => {
	beforeEach(() => vi.clearAllMocks());

	it('shows the organization area only after the live backend grants access', async () => {
		capability.get.mockResolvedValue({ allowed: true });
		render(OrganizationAdminLayout);
		await waitFor(() =>
			expect(screen.getByRole('heading', { name: 'Organization administration' })).toBeTruthy()
		);
		expect(capability.get).toHaveBeenCalledOnce();
		expect(screen.getByRole('link', { name: 'Groups' }).getAttribute('href')).toBe(
			'/organization-admin/groups'
		);
	});

	it.each([
		'ordinary member',
		'installation operator without a tenant grant',
		'revoked organization admin'
	])('fails closed for %s without revealing tenant details', async () => {
		capability.get.mockRejectedValue(Object.assign(new Error('Forbidden'), { status: 403 }));
		render(OrganizationAdminLayout);
		await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('not authorized'));
		expect(screen.queryByRole('navigation', { name: 'Organization administration' })).toBeNull();
		expect(document.body.textContent).not.toMatch(/tenant|organization id/i);
	});

	it('shows a retryable error when capability cannot be checked', async () => {
		capability.get.mockRejectedValue(new Error('offline'));
		render(OrganizationAdminLayout);
		await waitFor(() =>
			expect(
				screen.getByRole('heading', { name: 'Could not verify organization access' })
			).toBeTruthy()
		);
		expect(screen.queryByRole('navigation', { name: 'Organization administration' })).toBeNull();
	});

	it('rechecks a grant when a suspended tab becomes active', async () => {
		capability.get
			.mockResolvedValueOnce({ allowed: true })
			.mockRejectedValueOnce(Object.assign(new Error('Forbidden'), { status: 403 }));
		render(OrganizationAdminLayout);
		await waitFor(() =>
			expect(screen.getByRole('heading', { name: 'Organization administration' })).toBeTruthy()
		);
		await fireEvent.focus(window);
		await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('not authorized'));
		expect(capability.get).toHaveBeenCalledTimes(2);
	});
});
