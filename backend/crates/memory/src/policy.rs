//! Per-tenant projection policy for the Memory catalog.
//!
//! Flags are read from the chat Application's `configuration` JSONB object
//! (`application_enablements.configuration` for `io.elembra.chat`). Absent or
//! non-boolean values mean `false`: projection is OFF and content is not
//! indexed unless explicitly enabled.

use serde_json::Value;

use crate::event::ChatChannelKind;

/// Per-tenant flags controlling Memory projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectionPolicy {
    /// Master switch; `false` means no Memory projection at all. Default OFF.
    pub memory_projection: bool,
    /// Whether message bodies may be stored in the catalog. Default `false`.
    /// Never gates record existence — only body storage.
    pub content_indexing: bool,
}

/// Whether a channel event should be projected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionDecision {
    Project,
    Skip(SkipReason),
}

/// Why a channel event is not projected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The `memory_projection` flag is off.
    ProjectionDisabled,
    /// `dm` / `private` / `excluded` channels are never projected.
    ChannelNotEligible(ChatChannelKind),
}

impl ProjectionPolicy {
    /// Read flags from the chat Application's `configuration` JSONB object.
    ///
    /// Keys `memory_projection` and `content_indexing`; absent or non-bool
    /// values are treated as `false` (fail closed).
    pub fn from_config(config: &Value) -> Self {
        Self {
            memory_projection: flag(config, "memory_projection"),
            content_indexing: flag(config, "content_indexing"),
        }
    }

    /// Decide whether an event in `channel_kind` is projected.
    ///
    /// `dm` / `private` / `excluded` channels are skipped regardless of the
    /// flags; otherwise projection requires `memory_projection` to be on.
    /// `content_indexing` never gates record existence.
    pub fn decision(&self, channel_kind: ChatChannelKind) -> ProjectionDecision {
        match channel_kind {
            ChatChannelKind::Dm | ChatChannelKind::Private | ChatChannelKind::Excluded => {
                ProjectionDecision::Skip(SkipReason::ChannelNotEligible(channel_kind))
            }
            _ if !self.memory_projection => {
                ProjectionDecision::Skip(SkipReason::ProjectionDisabled)
            }
            _ => ProjectionDecision::Project,
        }
    }
}

fn flag(config: &Value, key: &str) -> bool {
    config.get(key).and_then(Value::as_bool).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_config_defaults_off_for_missing_and_empty() {
        assert_eq!(
            ProjectionPolicy::from_config(&Value::Null),
            ProjectionPolicy::default()
        );
        assert_eq!(
            ProjectionPolicy::from_config(&json!({})),
            ProjectionPolicy::default()
        );
        assert_eq!(
            ProjectionPolicy::from_config(&json!({"unrelated": true})),
            ProjectionPolicy::default()
        );
    }

    #[test]
    fn from_config_reads_explicit_bool_flags() {
        let policy = ProjectionPolicy::from_config(&json!({
            "memory_projection": true,
            "content_indexing": true,
        }));
        assert!(policy.memory_projection);
        assert!(policy.content_indexing);

        let partial = ProjectionPolicy::from_config(&json!({"memory_projection": true}));
        assert!(partial.memory_projection);
        assert!(!partial.content_indexing);
    }

    #[test]
    fn from_config_non_bool_values_fail_closed() {
        for value in [
            json!({"memory_projection": "yes"}),
            json!({"memory_projection": 1}),
            json!({"memory_projection": null}),
            json!({"memory_projection": []}),
        ] {
            assert!(!ProjectionPolicy::from_config(&value).memory_projection);
        }
        for value in [
            json!({"content_indexing": "yes"}),
            json!({"content_indexing": 1}),
            json!({"content_indexing": null}),
            json!({"content_indexing": {}}),
        ] {
            assert!(!ProjectionPolicy::from_config(&value).content_indexing);
        }
    }

    #[test]
    fn decision_skips_never_eligible_channels_regardless_of_flags() {
        for kind in [
            ChatChannelKind::Dm,
            ChatChannelKind::Private,
            ChatChannelKind::Excluded,
        ] {
            for memory_projection in [false, true] {
                let policy = ProjectionPolicy {
                    memory_projection,
                    content_indexing: true,
                };
                assert_eq!(
                    policy.decision(kind),
                    ProjectionDecision::Skip(SkipReason::ChannelNotEligible(kind)),
                    "kind={kind:?} memory_projection={memory_projection}"
                );
            }
        }
    }

    #[test]
    fn decision_workspace_requires_memory_projection() {
        let off = ProjectionPolicy::default();
        assert_eq!(
            off.decision(ChatChannelKind::Workspace),
            ProjectionDecision::Skip(SkipReason::ProjectionDisabled)
        );

        let on = ProjectionPolicy {
            memory_projection: true,
            content_indexing: false,
        };
        assert_eq!(
            on.decision(ChatChannelKind::Workspace),
            ProjectionDecision::Project
        );

        // content_indexing never gates existence.
        let indexed = ProjectionPolicy {
            memory_projection: true,
            content_indexing: true,
        };
        assert_eq!(
            indexed.decision(ChatChannelKind::Workspace),
            ProjectionDecision::Project
        );
    }
}
