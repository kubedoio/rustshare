-- Normalize legacy notification text values to canonical snake_case strings.
-- Older builds stored mixed-case values like "ShareReceived" and "File".

UPDATE notifications
SET notification_type = CASE LOWER(REPLACE(REPLACE(notification_type, '_', ''), '-', ''))
    WHEN 'sharereceived' THEN 'share_received'
    WHEN 'permissionchanged' THEN 'permission_changed'
    WHEN 'sharerevoked' THEN 'share_revoked'
    ELSE notification_type
END
WHERE notification_type <> CASE LOWER(REPLACE(REPLACE(notification_type, '_', ''), '-', ''))
    WHEN 'sharereceived' THEN 'share_received'
    WHEN 'permissionchanged' THEN 'permission_changed'
    WHEN 'sharerevoked' THEN 'share_revoked'
    ELSE notification_type
END;

UPDATE notifications
SET resource_type = CASE LOWER(TRIM(resource_type))
    WHEN 'file' THEN 'file'
    WHEN 'folder' THEN 'folder'
    WHEN 'share' THEN 'share'
    ELSE resource_type
END
WHERE resource_type <> CASE LOWER(TRIM(resource_type))
    WHEN 'file' THEN 'file'
    WHEN 'folder' THEN 'folder'
    WHEN 'share' THEN 'share'
    ELSE resource_type
END;
