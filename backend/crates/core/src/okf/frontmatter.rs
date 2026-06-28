use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Error type for OKF frontmatter parsing and serialization.
#[derive(Debug, thiserror::Error)]
pub enum FrontmatterError {
    /// The frontmatter block contained invalid YAML.
    #[error("invalid YAML frontmatter: {0}")]
    InvalidYaml(#[from] serde_yaml::Error),
    /// The frontmatter block parsed successfully but was not a YAML mapping.
    #[error("frontmatter is not a YAML mapping")]
    NotAMapping,
}

/// RustShare-specific nested frontmatter for a note.
///
/// All fields are optional so that partial frontmatter can be parsed and
/// merged. Unknown nested keys are preserved in `extra`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustshareFrontmatter {
    /// Stable identity of the note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    /// Module key, e.g. `notes`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Kind of source that owns the note, e.g. `workspace`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    /// Identifier of the source that owns the note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Name of the note bundle folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_name: Option<String>,
    /// Main Markdown file within the bundle, e.g. `note.md`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    /// Visibility policy, e.g. `private` or `workspace`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    /// Hash of the access-control list for this note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acl_hash: Option<String>,
    /// Embedding / indexing policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_policy: Option<String>,
    /// Verification status of the frontmatter against the bundle manifest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_status: Option<String>,
    /// Unknown nested keys preserved verbatim.
    #[serde(flatten, default)]
    pub extra: serde_json::Value,
}

impl Default for RustshareFrontmatter {
    fn default() -> Self {
        Self {
            id: None,
            module: None,
            source_kind: None,
            source_id: None,
            bundle_name: None,
            main: None,
            visibility: None,
            acl_hash: None,
            embedding_policy: None,
            verification_status: None,
            extra: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

/// Top-level OKF frontmatter for a Markdown note.
///
/// The `type` field is represented as `okf_type` in Rust because `type` is a
/// reserved keyword. All fields are optional so existing frontmatter can be
/// parsed and merged incrementally. Unknown top-level keys are preserved in
/// `extra`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkfNoteFrontmatter {
    /// OKF document type, e.g. `Note`.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub okf_type: Option<String>,
    /// Display title of the note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Short description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Resource locator / canonical reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    /// Tags attached to the note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Timestamp of the frontmatter revision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    /// RustShare-specific metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rustshare: Option<RustshareFrontmatter>,
    /// Unknown top-level keys preserved verbatim.
    #[serde(flatten, default)]
    pub extra: serde_json::Value,
}

impl Default for OkfNoteFrontmatter {
    fn default() -> Self {
        Self {
            okf_type: None,
            title: None,
            description: None,
            resource: None,
            tags: Vec::new(),
            timestamp: None,
            rustshare: None,
            extra: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

/// Returns true when `id` should be treated as a valid, non-placeholder
/// identity.
fn is_valid_uuid(id: Uuid) -> bool {
    !id.is_nil()
}

/// Detect and split a YAML frontmatter block from a Markdown document.
///
/// A frontmatter block must start with `---\n` at the very beginning of the
/// document and end with a `---` marker on its own line. The first such block
/// is considered the frontmatter; any later `---` blocks are left in the body.
///
/// On success returns `Some((yaml_string, markdown_body))`. If the document
/// does not start with frontmatter, returns `None`.
pub fn split_frontmatter(doc: &str) -> Option<(String, String)> {
    let without_prefix = doc.strip_prefix("---\n")?;
    let end = without_prefix.find("\n---")?;
    let after_marker = &without_prefix[end + 4..];

    // The closing marker must be followed by a newline or the end of input.
    if !after_marker.is_empty() && !after_marker.starts_with('\n') {
        return None;
    }

    let yaml = without_prefix[..end].to_string();
    let body = if after_marker.is_empty() {
        String::new()
    } else {
        after_marker[1..].to_string()
    };

    Some((yaml, body))
}

/// Parse a Markdown document into frontmatter and body.
///
/// If the document does not contain a frontmatter block, the returned
/// frontmatter is [`OkfNoteFrontmatter::default`] and the body is the original
/// document.
pub fn parse_frontmatter(doc: &str) -> Result<(OkfNoteFrontmatter, String), FrontmatterError> {
    if let Some((yaml, body)) = split_frontmatter(doc) {
        let fm: OkfNoteFrontmatter = serde_yaml::from_str(&yaml)?;
        Ok((fm, body))
    } else {
        Ok((OkfNoteFrontmatter::default(), doc.to_string()))
    }
}

/// Serialize frontmatter deterministically to a YAML string.
///
/// The returned string does **not** include the `---` wrapper markers, so it
/// can be embedded safely without double-wrapping.
pub fn serialize_frontmatter(fm: &OkfNoteFrontmatter) -> Result<String, FrontmatterError> {
    let yaml = serde_yaml::to_string(fm)?;
    Ok(yaml.trim_end_matches('\n').to_string())
}

/// Combine frontmatter and a Markdown body into a complete document.
///
/// The body is appended verbatim after the closing `---` marker.
pub fn to_document(
    frontmatter: &OkfNoteFrontmatter,
    body: &str,
) -> Result<String, FrontmatterError> {
    let yaml = serialize_frontmatter(frontmatter)?;
    Ok(format!("---\n{yaml}\n---\n{body}"))
}

/// Merge required RustShare OKF keys into existing frontmatter.
///
/// Missing top-level and nested fields are filled from `required`. A valid
/// existing `rustshare.id` (any non-nil UUID) is preserved. Unknown keys in
/// both `extra` maps are preserved, and required extra keys are added only
/// when not already present.
pub fn merge_required_okf_keys(
    existing: Option<OkfNoteFrontmatter>,
    required: OkfNoteFrontmatter,
) -> OkfNoteFrontmatter {
    let mut merged = existing.unwrap_or_default();

    if merged.okf_type.is_none() {
        merged.okf_type = required.okf_type.clone();
    }
    if merged.title.is_none() {
        merged.title = required.title.clone();
    }
    if merged.description.is_none() {
        merged.description = required.description.clone();
    }
    if merged.resource.is_none() {
        merged.resource = required.resource.clone();
    }
    if merged.tags.is_empty() {
        merged.tags.clone_from(&required.tags);
    }
    if merged.timestamp.is_none() {
        merged.timestamp = required.timestamp;
    }

    // Merge top-level unknown keys without overwriting.
    if let (Some(merged_obj), Some(req_obj)) =
        (merged.extra.as_object_mut(), required.extra.as_object())
    {
        for (k, v) in req_obj {
            if !merged_obj.contains_key(k) {
                merged_obj.insert(k.clone(), v.clone());
            }
        }
    }

    // Merge rustshare block.
    let required_rs = required.rustshare.unwrap_or_default();
    let mut merged_rs = merged.rustshare.take().unwrap_or_default();

    if merged_rs.id.is_none_or(|id| !is_valid_uuid(id)) {
        merged_rs.id = required_rs.id;
    }

    macro_rules! fill {
        ($field:ident) => {
            if merged_rs.$field.is_none() {
                merged_rs.$field.clone_from(&required_rs.$field);
            }
        };
    }

    fill!(module);
    fill!(source_kind);
    fill!(source_id);
    fill!(bundle_name);
    fill!(main);
    fill!(visibility);
    fill!(acl_hash);
    fill!(embedding_policy);
    fill!(verification_status);

    // Merge nested unknown keys without overwriting.
    if let (Some(merged_obj), Some(req_obj)) = (
        merged_rs.extra.as_object_mut(),
        required_rs.extra.as_object(),
    ) {
        for (k, v) in req_obj {
            if !merged_obj.contains_key(k) {
                merged_obj.insert(k.clone(), v.clone());
            }
        }
    }

    merged.rustshare = Some(merged_rs);
    merged
}

/// Build a default OKF frontmatter for a new note.
///
/// `workspace_id` is used as the default `source_id` when `source_id` is
/// supplied as an empty string; otherwise `source_id` is stored verbatim.
pub fn default_note_frontmatter(
    title: impl Into<String>,
    note_id: Uuid,
    workspace_id: Uuid,
    source_id: impl Into<String>,
    bundle_name: impl Into<String>,
    acl_hash: impl Into<String>,
) -> OkfNoteFrontmatter {
    let source_id = source_id.into();
    let source_id = if source_id.is_empty() {
        note_id.to_string()
    } else {
        source_id
    };

    OkfNoteFrontmatter {
        okf_type: Some("Note".to_string()),
        title: Some(title.into()),
        description: Some("".to_string()),
        resource: Some(format!(
            "rustshare://workspace/{}/notes/{}",
            workspace_id, note_id
        )),
        tags: Vec::new(),
        timestamp: Some(Utc::now()),
        rustshare: Some(RustshareFrontmatter {
            id: Some(note_id),
            module: Some("notes".to_string()),
            source_kind: Some("note".to_string()),
            source_id: Some(source_id),
            bundle_name: Some(bundle_name.into()),
            main: Some("note.md".to_string()),
            visibility: Some("private".to_string()),
            acl_hash: Some(acl_hash.into()),
            embedding_policy: Some("allowed".to_string()),
            verification_status: Some("draft".to_string()),
            extra: serde_json::Value::Object(serde_json::Map::new()),
        }),
        extra: serde_json::Value::Object(serde_json::Map::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE_ID: &str = "550e8400-e29b-41d4-a716-446655440000";
    const WORKSPACE_ID: &str = "660e8400-e29b-41d4-a716-446655440001";

    #[test]
    fn test_document_without_frontmatter() {
        let doc = "# Hello\n\nBody text.";
        let (fm, body) = parse_frontmatter(doc).unwrap();

        assert_eq!(fm.okf_type, None);
        assert_eq!(fm.title, None);
        assert!(fm.rustshare.is_none());
        assert_eq!(body, "# Hello\n\nBody text.");
    }

    #[test]
    fn test_document_with_valid_frontmatter() {
        let doc = format!(
            "---\n\
             type: Note\n\
             title: My Note\n\
             tags:\n  - alpha\n  - beta\n\
             rustshare:\n  id: {NOTE_ID}\n  module: notes\n\
             ---\n\
             # My Note\n"
        );
        let (fm, body) = parse_frontmatter(&doc).unwrap();

        assert_eq!(fm.okf_type, Some("Note".to_string()));
        assert_eq!(fm.title, Some("My Note".to_string()));
        assert_eq!(fm.tags, vec!["alpha", "beta"]);
        assert_eq!(
            fm.rustshare.as_ref().unwrap().id,
            Some(NOTE_ID.parse().unwrap())
        );
        assert_eq!(
            fm.rustshare.as_ref().unwrap().module,
            Some("notes".to_string())
        );
        assert_eq!(body, "# My Note\n");
    }

    #[test]
    fn test_unknown_fields_preserved() {
        let doc = format!(
            "---\n\
             type: Note\n\
             custom_top: hello\n\
             rustshare:\n  id: {NOTE_ID}\n  custom_nested: world\n\
             ---\nbody"
        );
        let (fm, _) = parse_frontmatter(&doc).unwrap();

        assert_eq!(
            fm.extra.get("custom_top"),
            Some(&serde_json::Value::String("hello".to_string()))
        );
        let rs = fm.rustshare.as_ref().unwrap();
        assert_eq!(
            rs.extra.get("custom_nested"),
            Some(&serde_json::Value::String("world".to_string()))
        );

        let yaml = serialize_frontmatter(&fm).unwrap();
        assert!(yaml.contains("custom_top:"));
        assert!(yaml.contains("custom_nested:"));
    }

    #[test]
    fn test_merge_preserves_existing_rustshare_id() {
        let existing = OkfNoteFrontmatter {
            rustshare: Some(RustshareFrontmatter {
                id: Some(NOTE_ID.parse().unwrap()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let required = default_note_frontmatter(
            "New Note",
            Uuid::new_v4(),
            WORKSPACE_ID.parse().unwrap(),
            "workspace-src",
            "new-bundle",
            "new-acl",
        );

        let merged = merge_required_okf_keys(Some(existing), required);
        let rs = merged.rustshare.unwrap();

        assert_eq!(rs.id, Some(NOTE_ID.parse().unwrap()));
        assert_eq!(rs.module, Some("notes".to_string()));
        assert_eq!(rs.bundle_name, Some("new-bundle".to_string()));
        assert_eq!(rs.acl_hash, Some("new-acl".to_string()));
    }

    #[test]
    fn test_invalid_yaml_returns_error() {
        let doc = "---\n\
                   type: [unclosed\n\
                   ---\nbody";
        let result = parse_frontmatter(doc);
        assert!(result.is_err(), "invalid YAML should produce an error");
    }

    #[test]
    fn test_duplicate_frontmatter_blocks_safe() {
        let doc = "---\n\
                   type: Note\n\
                   ---\n\
                   ---\n\
                   title: second\n\
                   ---\nbody";
        let (fm, body) = parse_frontmatter(doc).unwrap();

        assert_eq!(fm.okf_type, Some("Note".to_string()));
        assert_eq!(fm.title, None);
        assert_eq!(body, "---\ntitle: second\n---\nbody");
    }

    #[test]
    fn test_round_trip_preserves_unknown_keys() {
        let doc = format!(
            "---\n\
             type: Note\n\
             title: My Note\n\
             custom_top: hello\n\
             rustshare:\n  id: {NOTE_ID}\n  custom_nested: world\n\
             ---\n\
             # Body\n"
        );
        let (fm, body) = parse_frontmatter(&doc).unwrap();
        let doc2 = to_document(&fm, &body).unwrap();
        let (fm2, body2) = parse_frontmatter(&doc2).unwrap();

        assert_eq!(fm, fm2);
        assert_eq!(body, body2);
    }

    #[test]
    fn test_merge_does_not_double_wrap() {
        let existing_doc = "---\n\
                            type: Note\n\
                            title: Existing\n\
                            ---\n\
                            # Body";
        let (existing_fm, body) = parse_frontmatter(existing_doc).unwrap();
        let required = default_note_frontmatter(
            "Default",
            Uuid::new_v4(),
            WORKSPACE_ID.parse().unwrap(),
            "workspace-src",
            "bundle",
            "acl",
        );

        let merged = merge_required_okf_keys(Some(existing_fm), required);
        let doc = to_document(&merged, &body).unwrap();

        // Exactly two `---` markers: one opener and one closer.
        assert_eq!(doc.matches("---").count(), 2);
        assert!(doc.starts_with("---\n"));
        assert!(doc.contains("title: Existing"));
    }
}
