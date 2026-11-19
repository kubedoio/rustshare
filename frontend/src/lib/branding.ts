export type BrandingTheme = 'rustshare' | 'ocean' | 'forest';

export interface ProductBranding {
	productName: string;
	logoUrl: string | null;
	theme: BrandingTheme;
}

const DEFAULT_BRANDING: ProductBranding = {
	productName: 'RustShare',
	logoUrl: null,
	theme: 'rustshare'
};

const THEMES = new Set<BrandingTheme>(['rustshare', 'ocean', 'forest']);

function hasControlCharacters(value: string): boolean {
	return [...value].some((character) => {
		const code = character.charCodeAt(0);
		return code < 32 || code === 127;
	});
}

export function createProductBranding(config: Record<string, string | undefined>): ProductBranding {
	const productName = config.VITE_BRANDING_NAME?.trim();
	const logoUrl = config.VITE_BRANDING_LOGO_URL?.trim();
	const theme = config.VITE_BRANDING_THEME?.trim() as BrandingTheme | undefined;

	return {
		productName:
			productName && productName.length <= 60 && !hasControlCharacters(productName)
				? productName
				: DEFAULT_BRANDING.productName,
		logoUrl:
			logoUrl &&
			/^\/[A-Za-z0-9][A-Za-z0-9/_-]*(?:\.[A-Za-z0-9]+)?$/.test(logoUrl) &&
			!logoUrl.split('/').includes('..')
				? logoUrl
				: DEFAULT_BRANDING.logoUrl,
		theme: theme && THEMES.has(theme) ? theme : DEFAULT_BRANDING.theme
	};
}

// Build-time values are shared by every tenant in this deployment.
export const productBranding = createProductBranding({
	VITE_BRANDING_NAME: import.meta.env.VITE_BRANDING_NAME,
	VITE_BRANDING_LOGO_URL: import.meta.env.VITE_BRANDING_LOGO_URL,
	VITE_BRANDING_THEME: import.meta.env.VITE_BRANDING_THEME
});
