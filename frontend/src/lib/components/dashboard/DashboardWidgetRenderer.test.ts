import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import DashboardWidgetRenderer from './DashboardWidgetRenderer.svelte';

describe('DashboardWidgetRenderer', () => {
	it('falls back to GenericModuleSummaryWidget for unknown widget type', () => {
		const module = {
			id: 'module-unknown',
			key: 'unknown-module',
			displayName: 'Unknown Module',
			description: 'An unknown module.',
			enabled: true,
			rootPath: '/Workspace/Unknown',
			renderer: 'unknown',
			defaultTemplate: null,
			icon: 'folder',
			schemaVersion: '1.0',
			permissions: {
				adminCanConfigure: true,
				workspaceMembersCanUse: true,
				allowPublicShare: false,
				allowInternalShare: true
			},
			ui: {
				sidebar: { enabled: true, order: 99, icon: 'folder', label: 'Unknown' },
				dashboard: {
					enabled: true,
					order: 99,
					widget: {
						enabled: true,
						type: 'totally-unknown-widget-type',
						title: 'Unknown Module',
						description: 'An unknown module.',
						size: 'small' as const,
						columns: { desktop: 3, tablet: 6, mobile: 12 },
						maxItems: 4
					}
				},
				page: {
					enabled: true,
					route: '/modules/unknown-module',
					renderer: 'unknown',
					layout: 'generic-file-list',
					emptyStateTitle: 'No items yet',
					emptyStateDescription: 'Create the first item.',
					emptyStateAction: 'Create'
				}
			},
			aiIndexing: { enabled: true },
			audit: { enabled: true }
		};

		render(DashboardWidgetRenderer, { module, modules: [] });

		// GenericModuleSummaryWidget renders the module title
		expect(screen.getByText('Unknown Module')).toBeTruthy();
	});
});
