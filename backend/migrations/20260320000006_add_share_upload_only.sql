ALTER TABLE shares
ADD COLUMN IF NOT EXISTS upload_only BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE shares DROP CONSTRAINT IF EXISTS check_upload_only_folder_only;
ALTER TABLE shares
ADD CONSTRAINT check_upload_only_folder_only
CHECK (upload_only = FALSE OR folder_id IS NOT NULL);
