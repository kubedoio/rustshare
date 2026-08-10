DROP TRIGGER IF EXISTS users_disable_revokes_chat ON users;
DROP TRIGGER IF EXISTS users_delete_revokes_chat ON users;
DROP FUNCTION IF EXISTS revoke_chat_on_user_delete();
DROP FUNCTION IF EXISTS revoke_chat_on_user_disable();
