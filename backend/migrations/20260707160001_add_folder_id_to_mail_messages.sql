-- Add folder reference to imported mail messages so each artifact can live
-- inside a RustShare folder under /Workspace/Mail.
ALTER TABLE mail_messages
    ADD COLUMN folder_id UUID REFERENCES folders(id) ON DELETE SET NULL;
