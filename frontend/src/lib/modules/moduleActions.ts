import { goto } from '$app/navigation';
import { createFromTemplate } from '$lib/api/modules';
import type { ModuleConfig, PrimaryActionConfig } from '$lib/api/types';
import { getModuleObjectHref } from '$lib/modules/modulePages';

function moduleUrl(moduleKey: string): string {
	return `/modules/${moduleKey}`;
}

export async function runModulePrimaryAction(
	module: ModuleConfig,
	action: PrimaryActionConfig | null | undefined
): Promise<void> {
	if (!action) {
		await goto(moduleUrl(module.module_key));
		return;
	}

	switch (action.action) {
		case 'create-from-template': {
			const templateKey = action.template ?? module.default_template;
			if (!templateKey) {
				await goto(moduleUrl(module.module_key));
				return;
			}

			const created = await createFromTemplate({
				template_key: templateKey,
				name: buildDefaultObjectName(module, action.label)
			});

			await goto(getModuleObjectHref(module.module_key, created.object_type, created.object_id));
			return;
		}
		case 'open-module':
		case 'open-today-item':
		case 'manage-shares':
		default:
			await goto(moduleUrl(module.module_key));
	}
}

function buildDefaultObjectName(module: ModuleConfig, actionLabel: string): string {
	const normalizedLabel = actionLabel.trim().toLowerCase();
	if (normalizedLabel.startsWith('new ')) {
		return actionLabel.trim().slice(4).trim() || module.display_name;
	}

	return module.display_name;
}
