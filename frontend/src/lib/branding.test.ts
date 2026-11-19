import { describe, expect, it } from 'vitest';
import { createProductBranding } from './branding';

describe('deployment branding', () => {
	it('uses the product defaults when no branding is configured', () => {
		expect(createProductBranding({})).toEqual({
			productName: 'RustShare',
			logoUrl: null,
			theme: 'rustshare'
		});
	});

	it('falls back for invalid names, remote logos, and unapproved themes', () => {
		expect(
			createProductBranding({
				VITE_BRANDING_NAME: 'Bad\nName',
				VITE_BRANDING_LOGO_URL: 'https://example.com/logo.svg',
				VITE_BRANDING_THEME: 'url(javascript:alert(1))'
			})
		).toEqual({ productName: 'RustShare', logoUrl: null, theme: 'rustshare' });
	});

	it('uses one deployment branding configuration across tenants', () => {
		const deployedConfig = {
			VITE_BRANDING_NAME: 'Example Org',
			VITE_BRANDING_LOGO_URL: '/branding/logo.svg',
			VITE_BRANDING_THEME: 'ocean'
		};
		const firstLoad = createProductBranding({ ...deployedConfig, tenant_id: 'tenant-a' });
		const reloaded = createProductBranding({ ...deployedConfig, tenant_id: 'tenant-b' });

		expect(firstLoad).toEqual(reloaded);
		expect(firstLoad).toEqual({
			productName: 'Example Org',
			logoUrl: '/branding/logo.svg',
			theme: 'ocean'
		});
	});
});
