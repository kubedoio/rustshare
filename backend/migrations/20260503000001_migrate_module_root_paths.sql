-- Migrate existing module root paths from legacy direct-root format
-- to the new /Workspace/<Module> format.
-- Only updates rows that still use the old pattern.
UPDATE modules
SET root_path = '/Workspace/Notes'
WHERE root_path = '/Notes';

UPDATE modules
SET root_path = '/Workspace/Meetings'
WHERE root_path = '/Meetings';

UPDATE modules
SET root_path = '/Workspace/Standups'
WHERE root_path = '/Standups';

UPDATE modules
SET root_path = '/Workspace/Kanban'
WHERE root_path = '/Kanban';

UPDATE modules
SET root_path = '/Workspace/Decisions'
WHERE root_path = '/Decisions';

UPDATE modules
SET root_path = '/Workspace/Brainstorming'
WHERE root_path = '/Brainstorming';

UPDATE modules
SET root_path = '/Workspace/Shares'
WHERE root_path = '/Shares';
