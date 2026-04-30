import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ModuleCard from './ModuleCard.svelte';

vi.mock('$lib/api/modules', () => ({
	getModuleSummary: vi.fn()
}));

vi.mock('$lib/modules/moduleActions', () => ({
	runModulePrimaryAction: vi.fn()
}));

import { getModuleSummary } from '$lib/api/modules';
import { runModulePrimaryAction } from '$lib/modules/moduleActions';

const notesModule = {
	id: 'module-notes',
	module_key: 'notes',
	display_name: 'Notes',
	description: 'Recent notes',
	enabled: true,
	root_path: '/Notes',
	renderer: 'notes',
	default_template: 'template_default_note',
	icon: 'sticky-note',
	schema_version: '1',
	permissions: {
		admin_can_configure: true,
		workspace_members_can_use: true,
		allow_public_share: true,
		allow_internal_share: true
	},
	ai_indexing: { enabled: true },
	audit: { enabled: true },
	ui_config: {
		dashboard: {
			enabled: true,
			order: 10,
			cardTitle: 'Notes',
			cardDescription: 'Recent notes',
			summaryMode: 'recent-items',
			maxItems: 2,
			primaryAction: {
				label: 'New Note',
				action: 'create-from-template',
				template: 'template_default_note'
			}
		}
	},
	created_at: '2026-04-30T00:00:00Z',
	updated_at: '2026-04-30T00:00:00Z'
};

describe('ModuleCard', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('respects dashboard maxItems when rendering recent items', async () => {
		vi.mocked(getModuleSummary).mockResolvedValue({
			module_key: 'notes',
			mode: 'recent-items',
			total_items: 4,
			recent_items: [
				{ id: '1', name: 'Alpha', item_type: 'file', updated_at: '2026-04-30T00:00:00Z' },
				{ id: '2', name: 'Beta', item_type: 'file', updated_at: '2026-04-29T00:00:00Z' },
				{ id: '3', name: 'Gamma', item_type: 'file', updated_at: '2026-04-28T00:00:00Z' },
				{ id: '4', name: 'Delta', item_type: 'file', updated_at: '2026-04-27T00:00:00Z' }
			]
		});

		render(ModuleCard, {
			props: {
				module: notesModule
			}
		});

		await waitFor(() => {
			expect(screen.getByText('Alpha')).toBeTruthy();
		});

		expect(screen.getByText('Beta')).toBeTruthy();
		expect(screen.queryByText('Gamma')).toBeNull();
		expect(screen.queryByText('Delta')).toBeNull();
	});

	it('executes the configured primary action from the card button', async () => {
		vi.mocked(getModuleSummary).mockResolvedValue({
			module_key: 'notes',
			mode: 'recent-items',
			total_items: 0,
			recent_items: []
		});

		render(ModuleCard, {
			props: {
				module: notesModule
			}
		});

		await fireEvent.click(screen.getByRole('button', { name: /new note in notes/i }));

		expect(runModulePrimaryAction).toHaveBeenCalledWith(
			notesModule,
			notesModule.ui_config.dashboard.primaryAction
		);
	});

	it('shows share counts from specialized summary data', async () => {
		vi.mocked(getModuleSummary).mockResolvedValue({
			module_key: 'shares',
			mode: 'shares-overview',
			total_items: 2,
			recent_items: [
				{ id: '1', name: 'Launch Assets', item_type: 'folder', updated_at: '2026-04-30T00:00:00Z' }
			],
			extra: { publicCount: 3, internalCount: 5 }
		});

		render(ModuleCard, {
			props: {
				module: {
					id: 'module-shares',
					module_key: 'shares',
					display_name: 'Shares',
					description: 'Share packages',
					enabled: true,
					root_path: '/Shares',
					renderer: 'shares',
					default_template: 'template_default_share',
					icon: 'share-2',
					schema_version: '1',
					permissions: {
						admin_can_configure: true,
						workspace_members_can_use: true,
						allow_public_share: true,
						allow_internal_share: true
					},
					ai_indexing: { enabled: true },
					audit: { enabled: true },
					ui_config: {
						dashboard: {
							enabled: true,
							order: 60,
							cardTitle: 'Shares',
							cardDescription: 'Share packages',
							summaryMode: 'shares-overview',
							maxItems: 4
						}
					},
					created_at: '2026-04-30T00:00:00Z',
					updated_at: '2026-04-30T00:00:00Z'
				}
			}
		});

		await waitFor(() => {
			expect(screen.getByText('3 public, 5 internal')).toBeTruthy();
		});
	});
});
