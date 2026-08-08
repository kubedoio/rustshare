import { goto } from '$app/navigation';
import { createFromTemplate } from '$lib/api/applications';
import type { ApplicationConfig, PrimaryActionConfig } from '$lib/api/types';
import { getApplicationObjectHref } from '$lib/applications/applicationPages';

function moduleUrl(applicationId: string): string {
	return `/apps/${applicationId}`;
}

export async function runApplicationPrimaryAction(
	module: ApplicationConfig,
	action: PrimaryActionConfig | null | undefined
): Promise<void> {
	if (!action) {
		await goto(moduleUrl(module.application_id));
		return;
	}

	switch (action.action) {
		case 'create-from-template': {
			const templateKey = action.template ?? module.default_template;
			if (!templateKey) {
				await goto(moduleUrl(module.application_id));
				return;
			}

			const created = await createFromTemplate({
				template_key: templateKey,
				name: buildDefaultObjectName(module, action.label)
			});

			await goto(
				getApplicationObjectHref(module.application_id, created.object_type, created.object_id)
			);
			return;
		}
		case 'open-module':
		case 'open-today-item':
		case 'manage-shares':
		default:
			await goto(moduleUrl(module.application_id));
	}
}

function buildDefaultObjectName(module: ApplicationConfig, actionLabel: string): string {
	const normalizedLabel = actionLabel.trim().toLowerCase();
	if (normalizedLabel.startsWith('new ')) {
		return actionLabel.trim().slice(4).trim() || module.display_name;
	}

	return module.display_name;
}
