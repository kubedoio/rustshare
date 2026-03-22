import { defineConfig } from '@playwright/test';

export default defineConfig({
	webServer: process.env.E2E_BASE_URL
		? undefined
		: { command: 'npm run build && npm run preview', port: 4173 },
	use: {
		baseURL: process.env.E2E_BASE_URL ?? 'http://localhost:4173'
	},
	testMatch: '**/*.e2e.{ts,js}'
});
