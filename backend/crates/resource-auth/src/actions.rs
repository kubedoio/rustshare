//! Canonical action capability names (ADR-0032, `docs/specs/resource-ref-authorization-v1alpha1.md`).
//!
//! Action capabilities are stable dotted strings owned by Applications:
//! `<application-namespace>.<verb>`. The owning Application defines the
//! semantics of each action on its resource types.
//!
//! Only `files.*` actions are implemented in this phase; the other namespaces
//! are reserved so consumers and delegation records use stable identifiers
//! from day one.

/// Read a Files resource (file metadata, content, versions).
pub const FILES_READ: &str = "files.read";
/// Write/mutate a Files resource (create, update, rename, move, restore).
pub const FILES_WRITE: &str = "files.write";
/// Delete a Files resource (soft delete; folder delete covers the subtree).
pub const FILES_DELETE: &str = "files.delete";
/// Manage sharing of a Files resource (create, update, revoke shares).
pub const FILES_SHARE: &str = "files.share";

/// Reserved for Elembra Notes.
pub const NOTES_READ: &str = "notes.read";
/// Reserved for Elembra Notes.
pub const NOTES_WRITE: &str = "notes.write";
/// Reserved for Elembra Mail.
pub const MAIL_READ: &str = "mail.read";
/// Reserved for Elembra Mail.
pub const MAIL_SEND: &str = "mail.send";
/// Reserved for Elembra Chat.
pub const CHAT_READ: &str = "chat.read";
/// Reserved for Elembra Chat.
pub const CHAT_POST: &str = "chat.post";
/// Reserved for Elembra Memory.
pub const MEMORY_QUERY: &str = "memory.query";
/// Reserved for Elembra Agents.
pub const AGENTS_RUN: &str = "agents.run";
