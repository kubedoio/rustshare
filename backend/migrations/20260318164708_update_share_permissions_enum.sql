-- Migration: Update SharePermissions enum values
-- BREAKING CHANGE: Renames Read->View, ReadWrite->Edit, adds Admin

-- Step 1: Rename Read to View
UPDATE shares SET permissions = 'View' WHERE permissions = 'Read';

-- Step 2: Rename ReadWrite to Edit
UPDATE shares SET permissions = 'Edit' WHERE permissions = 'ReadWrite';

-- Step 3: Update CHECK constraint to allow Admin
ALTER TABLE shares DROP CONSTRAINT IF EXISTS check_permissions;
ALTER TABLE shares ADD CONSTRAINT check_permissions
  CHECK (permissions IN ('View', 'Edit', 'Admin'));

-- Verify no orphaned permission values
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM shares
    WHERE permissions NOT IN ('View', 'Edit', 'Admin')
  ) THEN
    RAISE EXCEPTION 'Found invalid permission values after migration';
  END IF;
END $$;
