-- Migration: Fix multi-tenant unique constraints on modules and templates

-- 1. Drop foreign key constraint on templates table
ALTER TABLE templates DROP CONSTRAINT IF EXISTS templates_module_key_fkey;

-- 2. Drop unique constraint modules_module_key_key on modules table
ALTER TABLE modules DROP CONSTRAINT IF EXISTS modules_module_key_key;

-- 3. Drop unique constraint templates_template_key_key on templates table
ALTER TABLE templates DROP CONSTRAINT IF EXISTS templates_template_key_key;

-- 4. Add composite unique constraint on modules(module_key, tenant_id)
ALTER TABLE modules ADD CONSTRAINT modules_module_key_tenant_id_key UNIQUE (module_key, tenant_id);

-- 5. Add composite unique constraint on templates(template_key, tenant_id)
ALTER TABLE templates ADD CONSTRAINT templates_template_key_tenant_id_key UNIQUE (template_key, tenant_id);

-- 6. Re-create foreign key constraint on templates referencing modules (composite)
ALTER TABLE templates ADD CONSTRAINT templates_module_key_fkey 
    FOREIGN KEY (module_key, tenant_id) 
    REFERENCES modules (module_key, tenant_id) 
    ON DELETE CASCADE;
