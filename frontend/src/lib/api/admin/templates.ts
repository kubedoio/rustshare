import { PUBLIC_API_URL } from '$env/static/public';
import { getAuthHeaders } from '../auth';

export interface TemplateDefinition {
    id: string;
    template_key: string;
    name: string;
    module_key: string;
    version: string;
    description: string;
    ui_config: any;
    folder_structure: string[];
    default_files: any[];
    metadata_schema: any;
    renderer?: string;
    visibility_policy: string;
    ai_indexing_policy: any;
    audit_logging_policy: any;
    enabled: boolean;
    system_template: boolean;
    created_at: string;
    updated_at: string;
}

export async function listTemplates(): Promise<TemplateDefinition[]> {
    const res = await fetch(`${PUBLIC_API_URL}/api/v1/admin/templates`, {
        headers: getAuthHeaders()
    });

    if (!res.ok) {
        throw new Error('Failed to fetch templates');
    }

    return res.json();
}

export async function getTemplate(key: string): Promise<TemplateDefinition> {
    const res = await fetch(`${PUBLIC_API_URL}/api/v1/admin/templates/${key}`, {
        headers: getAuthHeaders()
    });

    if (!res.ok) {
        throw new Error('Failed to fetch template');
    }

    return res.json();
}

export async function createTemplate(template: Partial<TemplateDefinition>): Promise<TemplateDefinition> {
    const res = await fetch(`${PUBLIC_API_URL}/api/v1/admin/templates`, {
        method: 'POST',
        headers: {
            ...getAuthHeaders(),
            'Content-Type': 'application/json'
        },
        body: JSON.stringify(template)
    });

    if (!res.ok) {
        const error = await res.json();
        throw new Error(error.error || 'Failed to create template');
    }

    return res.json();
}

export async function updateTemplate(key: string, template: Partial<TemplateDefinition>): Promise<TemplateDefinition> {
    const res = await fetch(`${PUBLIC_API_URL}/api/v1/admin/templates/${key}`, {
        method: 'PUT',
        headers: {
            ...getAuthHeaders(),
            'Content-Type': 'application/json'
        },
        body: JSON.stringify(template)
    });

    if (!res.ok) {
        const error = await res.json();
        throw new Error(error.error || 'Failed to update template');
    }

    return res.json();
}

export async function deleteTemplate(key: string): Promise<void> {
    const res = await fetch(`${PUBLIC_API_URL}/api/v1/admin/templates/${key}`, {
        method: 'DELETE',
        headers: getAuthHeaders()
    });

    if (!res.ok) {
        throw new Error('Failed to delete template');
    }
}

export async function duplicateTemplate(key: string): Promise<TemplateDefinition> {
    const res = await fetch(`${PUBLIC_API_URL}/api/v1/admin/templates/${key}/duplicate`, {
        method: 'POST',
        headers: getAuthHeaders()
    });

    if (!res.ok) {
        throw new Error('Failed to duplicate template');
    }

    return res.json();
}
