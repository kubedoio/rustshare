import { describe, expect, it } from 'vitest';
import {
	applicationConfigToDefinition,
	applicationShellEntryToConfig,
	getApplicationByRouteSlug,
	applicationsStore
} from './registry';
import { getEnabledSidebarApplications } from './workspaceSurface';

const manifest = {
	apiVersion: 'elembra.io/v1alpha1',
	kind: 'Application' as const,
	metadata: {
		id: 'io.elembra.notes',
		name: 'Notes',
		version: '1.0.0',
		description: 'Notes.'
	},
	runtime: { kind: 'embedded' as const },
	contracts: { provides: [], requires: [] },
	resources: [],
	contributions: {
		navigation: [{ id: 'notes.navigation', label: 'Notes', route: '/apps/notes', order: 10 }],
		routes: [{ id: 'notes.page', route: '/apps/notes', renderer: 'okf-note' }],
		commands: [],
		dashboard: [{ id: 'notes.dashboard', renderer: 'latest-notes', order: 10 }],
		settings: [{ id: 'notes.settings', label: 'Notes settings', route: '/settings/apps/notes' }],
		searchProviders: [],
		renderers: [],
		admin: []
	},
	integrationEvents: { publishes: [], subscribes: [] },
	configuration: { schema: 'schemas/io.elembra.notes.json' },
	data: { owner: 'io.elembra.notes', preserveOnDisable: true, exportSupported: true }
};

describe('Application registry', () => {
	it('derives identity and route slug from the manifest', () => {
		const definition = applicationConfigToDefinition(
			applicationShellEntryToConfig({
				manifest,
				enabled: true,
				configuration: {},
				health: 'healthy'
			})
		);

		expect(definition.id).toBe('io.elembra.notes');
		expect(definition.key).toBe('notes');
		expect(definition.ui.page.route).toBe('/apps/notes');
		expect(definition.renderer).toBe('okf-note');
		expect(definition.settings?.[0].route).toBe('/settings/apps/notes');
	});

	it('maps disabled state without inventing a second application catalogue', () => {
		const definition = applicationConfigToDefinition(
			applicationShellEntryToConfig({
				manifest,
				enabled: false,
				configuration: {},
				health: 'unavailable'
			})
		);

		expect(definition.enabled).toBe(false);
		expect(definition.id).toBe('io.elembra.notes');
	});

	it('resolves shell applications by declared route slug', () => {
		applicationsStore.set([
			applicationConfigToDefinition(
				applicationShellEntryToConfig({
					manifest,
					enabled: true,
					configuration: {},
					health: 'healthy'
				})
			)
		]);

		expect(getApplicationByRouteSlug('notes')?.id).toBe('io.elembra.notes');
		expect(getApplicationByRouteSlug('io.elembra.notes')).toBeUndefined();
	});

	it('renders a future Application from its navigation Contribution', () => {
		const futureManifest = {
			...manifest,
			metadata: { ...manifest.metadata, id: 'io.elembra.chat', name: 'Chat' },
			runtime: { kind: 'bridge' as const },
			contributions: {
				...manifest.contributions,
				navigation: [{ id: 'chat.navigation', label: 'Chat', route: '/apps/chat', order: 50 }],
				routes: [{ id: 'chat.route', route: '/apps/chat', renderer: 'chat' }]
			}
		};
		const config = applicationShellEntryToConfig({
			manifest: futureManifest,
			enabled: true,
			configuration: {},
			health: 'healthy'
		});

		const navigation = getEnabledSidebarApplications([config]);

		expect(config.application_id).toBe('io.elembra.chat');
		expect(navigation).toHaveLength(1);
		expect(navigation[0].application_id).toBe('io.elembra.chat');
		expect(navigation[0].ui_config?.sidebar?.label).toBe('Chat');
		expect(navigation[0].ui_config?.page?.route).toBe('/apps/chat');
	});
});
