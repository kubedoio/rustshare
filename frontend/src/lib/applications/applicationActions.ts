import { goto } from '$app/navigation';
import { createFromTemplate } from '$lib/api/applications';
import type { ApplicationConfig, PrimaryActionConfig } from '$lib/api/types';
import { getApplicationObjectHref } from '$lib/applications/applicationPages';

function applicationUrl(applicationId: string): string {
	return `/apps/${applicationId.split('.').at(-1) ?? applicationId}`;
}

export async function runApplicationPrimaryAction(
	application: ApplicationConfig,
	action: PrimaryActionConfig | null | undefined
): Promise<void> {
	if (!action) {
		await goto(applicationUrl(application.application_id));
		return;
	}

	switch (action.action) {
		case 'create-from-template': {
			const templateKey = action.template ?? application.default_template;
			if (!templateKey) {
				await goto(applicationUrl(application.application_id));
				return;
			}

			const created = await createFromTemplate({
				template_key: templateKey,
				name: buildDefaultObjectName(application, action.label)
			});

			await goto(
				getApplicationObjectHref(application.application_id, created.object_type, created.object_id)
			);
			return;
		}
		case 'open-module':
		case 'open-today-item':
		case 'manage-shares':
		default:
			await goto(applicationUrl(application.application_id));
	}
}

function buildDefaultObjectName(application: ApplicationConfig, actionLabel: string): string {
	const normalizedLabel = actionLabel.trim().toLowerCase();
	if (normalizedLabel.startsWith('new ')) {
		return actionLabel.trim().slice(4).trim() || application.display_name;
	}

	return application.display_name;
}
