-- Migration: Backfill tenant_id in notifications table from the corresponding user's tenant_id

UPDATE notifications n
SET tenant_id = u.tenant_id
FROM users u
WHERE n.user_id = u.id AND n.tenant_id = '00000000-0000-0000-0000-000000000000';
