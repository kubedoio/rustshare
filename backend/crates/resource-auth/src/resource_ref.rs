//! [`ResourceRef`] — a transport-neutral opaque reference to an Application-owned resource.
//!
//! See ADR-0032 and `docs/specs/resource-ref-authorization-v1alpha1.md`.
//!
//! A `ResourceRef` identifies a resource. It never grants access to it. Runtime
//! deployment strategy, database table names, storage keys, service URLs and
//! presigned URLs are explicitly forbidden as identity fields.

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use rustshare_core::domain::ApplicationId;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Maximum length of an opaque resource ID.
pub const MAX_RESOURCE_ID_LEN: usize = 512;
/// Maximum length of an Application-owned resource type name.
pub const MAX_RESOURCE_TYPE_LEN: usize = 64;
/// Maximum length of an opaque version selector.
pub const MAX_VERSION_LEN: usize = 256;

/// Percent-encode set for the canonical URI: everything except RFC 3986
/// unreserved characters (`A-Z a-z 0-9 - . _ ~`).
const URI_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// A cross-Application reference to an Application-owned resource.
///
/// Canonical JSON shape:
///
/// ```json
/// {
///   "application": "io.elembra.files",
///   "resourceType": "file",
///   "resourceId": "01K2ABC...",
///   "version": "sha256:0123..."
/// }
/// ```
///
/// Canonical URI rendering:
///
/// ```text
/// elembra://io.elembra.files/file/01K2ABC...?version=sha256%3A0123...
/// ```
///
/// Consumers treat the ref as opaque: they must not infer database table
/// names, storage keys, runtime topology or authorization from it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResourceRef {
    /// Stable Application ID that owns/resolves the resource.
    pub application: ApplicationId,
    /// Application-owned stable resource type, e.g. `file` or `folder`.
    #[serde(rename = "resourceType")]
    pub resource_type: String,
    /// Opaque resource identifier interpreted only by the owner.
    ///
    /// Owners MUST treat this as an opaque, untrusted value and never
    /// interpret it structurally (no path, URL, storage-key or SQL meaning).
    /// It is percent-encoded in the canonical URI, so a decoded value may
    /// legitimately contain reserved characters such as `/`, `?` or `#`.
    #[serde(rename = "resourceId")]
    pub resource_id: String,
    /// Optional immutable/version selector for provenance or historical access.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl ResourceRef {
    /// Create a ref without a version selector.
    pub fn new(
        application: ApplicationId,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        Self {
            application,
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            version: None,
        }
    }

    /// Create a ref with an immutable version selector.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Validate syntactic shape. Unknown Application/type knowledge is a
    /// dispatch-time concern handled by [`crate::SourceAuthorizer`].
    pub fn validate(&self) -> Result<(), String> {
        if !valid_namespace(&self.application.0) {
            return Err(format!("invalid application id `{}`", self.application));
        }
        if self.resource_type.is_empty()
            || self.resource_type.len() > MAX_RESOURCE_TYPE_LEN
            || !valid_namespace(&self.resource_type)
        {
            return Err(format!("invalid resource type `{}`", self.resource_type));
        }
        validate_opaque_segment(&self.resource_id, MAX_RESOURCE_ID_LEN, "resource id")?;
        if let Some(version) = &self.version {
            validate_opaque_segment(version, MAX_VERSION_LEN, "version")?;
            validate_version_shape(version)?;
        }
        Ok(())
    }

    /// Render the canonical URI form.
    ///
    /// ```text
    /// elembra://io.elembra.files/file/01K2ABC...?version=sha256%3A0123...
    /// ```
    pub fn to_uri(&self) -> String {
        let mut uri = format!(
            "elembra://{}/{}/{}",
            self.application,
            encode_segment(&self.resource_type),
            encode_segment(&self.resource_id)
        );
        if let Some(version) = &self.version {
            uri.push_str("?version=");
            uri.push_str(&encode_segment(version));
        }
        uri
    }

    /// Parse the canonical URI form. Unknown query parameters, fragments,
    /// userinfo and ports are rejected.
    pub fn from_uri(input: &str) -> Result<Self, RefParseError> {
        let rest = input
            .strip_prefix("elembra://")
            .ok_or_else(|| RefParseError::Malformed("missing `elembra://` scheme prefix".into()))?;
        if rest.is_empty() {
            return Err(RefParseError::Malformed("empty reference".into()));
        }
        if rest.contains('#') {
            return Err(RefParseError::Malformed(
                "fragments are not allowed in a resource reference".into(),
            ));
        }
        let (path, query) = match rest.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (rest, None),
        };
        let segments: Vec<&str> = path.split('/').collect();
        if segments.len() != 3 || segments.iter().any(|s| s.is_empty()) {
            return Err(RefParseError::Malformed(
                "path must be exactly `/<application>/<resource_type>/<resource_id>`".into(),
            ));
        }
        for segment in &segments {
            validate_percent_escapes(segment)?;
        }
        let application = ApplicationId::new(decode_segment(segments[0])?);
        let resource_type = decode_segment(segments[1])?;
        let resource_id = decode_segment(segments[2])?;

        let mut version = None;
        if let Some(query) = query {
            if query.is_empty() {
                return Err(RefParseError::Malformed("empty query string".into()));
            }
            for pair in query.split('&') {
                let (key, value) = pair.split_once('=').ok_or_else(|| {
                    RefParseError::Malformed(format!("query pair `{pair}` has no value"))
                })?;
                if key != "version" {
                    return Err(RefParseError::Malformed(format!(
                        "unknown query parameter `{key}`"
                    )));
                }
                if version.is_some() {
                    return Err(RefParseError::Malformed(
                        "duplicate `version` query parameter".into(),
                    ));
                }
                validate_percent_escapes(value)?;
                version = Some(decode_segment(value)?);
            }
        }

        let reference = ResourceRef {
            application,
            resource_type,
            resource_id,
            version,
        };
        reference.validate().map_err(RefParseError::Invalid)?;
        Ok(reference)
    }
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_uri())
    }
}

impl FromStr for ResourceRef {
    type Err = RefParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_uri(s)
    }
}

/// Errors raised while parsing a `ResourceRef` from its canonical URI form.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RefParseError {
    #[error("malformed resource reference: {0}")]
    Malformed(String),
    #[error("invalid resource reference: {0}")]
    Invalid(String),
}

/// Namespace syntax shared by Application IDs and resource types: one or more
/// dot-separated segments of ASCII lowercase letters, digits, `-` and `_`.
fn valid_namespace(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        })
}

fn validate_opaque_segment(value: &str, max_len: usize, what: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{what} must not be empty"));
    }
    if value.len() > max_len {
        return Err(format!("{what} exceeds maximum length {max_len}"));
    }
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(format!("{what} contains whitespace or control characters"));
    }
    Ok(())
}

/// Version selectors are opaque owner-defined strings of the form
/// `prefix:value`, e.g. `sha256:0123...` or `drive-revision:...`.
fn validate_version_shape(version: &str) -> Result<(), String> {
    let (prefix, value) = version
        .split_once(':')
        .ok_or_else(|| format!("version `{version}` must be `prefix:value`"))?;
    if prefix.is_empty()
        || !prefix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || value.is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'))
    {
        return Err(format!(
            "version `{version}` uses characters outside the `prefix:value` shape"
        ));
    }
    Ok(())
}

fn encode_segment(value: &str) -> String {
    utf8_percent_encode(value, URI_SEGMENT_ENCODE_SET).to_string()
}

fn decode_segment(value: &str) -> Result<String, RefParseError> {
    percent_decode_str(value)
        .decode_utf8()
        .map(|cow| cow.into_owned())
        .map_err(|_| RefParseError::Malformed(format!("segment `{value}` is not valid UTF-8")))
}

/// Reject malformed percent-escapes: every `%` must be followed by exactly
/// two hexadecimal digits, otherwise the canonical form is ambiguous.
fn validate_percent_escapes(value: &str) -> Result<(), RefParseError> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3).ok_or_else(|| {
                RefParseError::Malformed(format!("truncated percent-escape in `{value}`"))
            })?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return Err(RefParseError::Malformed(format!(
                    "malformed percent-escape `{value}`"
                )));
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::ApplicationId;

    fn sample_ref() -> ResourceRef {
        ResourceRef::new(ApplicationId::new("io.elembra.files"), "file", "01K2ABC123")
            .with_version("sha256:0123abcdef")
    }

    #[test]
    fn json_round_trip_preserves_all_fields() {
        let reference = sample_ref();
        let json = serde_json::to_string(&reference).unwrap();
        let parsed: ResourceRef = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reference);
        let value = serde_json::to_value(&reference).unwrap();
        assert_eq!(value["application"], "io.elembra.files");
        assert_eq!(value["resourceType"], "file");
        assert_eq!(value["resourceId"], "01K2ABC123");
        assert_eq!(value["version"], "sha256:0123abcdef");
    }

    #[test]
    fn json_without_version_omits_the_field() {
        let reference =
            ResourceRef::new(ApplicationId::new("io.elembra.files"), "file", "01K2ABC123");
        let json = serde_json::to_string(&reference).unwrap();
        assert!(!json.contains("version"));
        let parsed: ResourceRef = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reference);
    }

    #[test]
    fn uri_round_trip_with_version() {
        let reference = sample_ref();
        let uri = reference.to_uri();
        assert_eq!(
            uri,
            "elembra://io.elembra.files/file/01K2ABC123?version=sha256%3A0123abcdef"
        );
        let parsed = ResourceRef::from_uri(&uri).unwrap();
        assert_eq!(parsed, reference);
    }

    #[test]
    fn uri_round_trip_without_version() {
        let reference = ResourceRef::new(
            ApplicationId::new("io.elembra.files"),
            "folder",
            "01K2FOLDER99",
        );
        let parsed = ResourceRef::from_uri(&reference.to_uri()).unwrap();
        assert_eq!(parsed, reference);
    }

    #[test]
    fn uri_percent_encodes_opaque_ids() {
        let reference = ResourceRef::new(
            ApplicationId::new("io.elembra.files"),
            "file",
            "external:id/with:chars",
        );
        let uri = reference.to_uri();
        // Opaque ids may contain reserved characters; the canonical URI
        // percent-encodes them so segments stay unambiguous.
        assert!(uri.contains("%2F"), "slash must be percent-encoded");
        assert!(uri.contains("%3A"), "colon must be percent-encoded");
        assert!(!uri.contains("with:chars"), "raw opaque id must not leak");
        let parsed = ResourceRef::from_uri(&uri).unwrap();
        assert_eq!(parsed, reference);
    }

    #[test]
    fn runtime_kind_is_not_part_of_ref_identity() {
        // A ref is pure identity: it carries no runtime, endpoint, table or
        // storage-key fields. Two refs for the same resource serialize to the
        // same value regardless of how the owning Application is deployed.
        let reference = sample_ref();
        let json = serde_json::to_value(&reference).unwrap();
        let object = json.as_object().unwrap();
        assert_eq!(object.len(), 4);
        for key in [
            "runtime",
            "endpoint",
            "table",
            "storageKey",
            "hostname",
            "strategy",
        ] {
            assert!(
                !object.contains_key(key),
                "unexpected identity field `{key}`"
            );
        }
    }

    #[test]
    fn malformed_uris_fail_closed() {
        let cases = [
            "https://io.elembra.files/file/abc",         // wrong scheme
            "elembra:///file/abc",                       // missing application
            "elembra://io.elembra.files",                // missing type/id
            "elembra://io.elembra.files/file",           // missing id
            "elembra://io.elembra.files/file/abc/",      // trailing slash
            "elembra://io.elembra.files//abc",           // empty type
            "elembra://io.elembra.files/file/abc?",      // empty query
            "elembra://io.elembra.files/file/abc?foo=1", // unknown query param
            "elembra://io.elembra.files/file/abc?version=a&version=b", // duplicate
            "elembra://user@io.elembra.files/file/abc",  // userinfo
            "elembra://io.elembra.files/file/abc#frag",  // fragment rejected as path
            "elembra://io.elembra.files/file/%ZZ",       // malformed percent-escape
            "elembra://io.elembra.files/file/abc?version=%2", // truncated escape
        ];
        for case in cases {
            assert!(
                ResourceRef::from_uri(case).is_err(),
                "expected `{case}` to be rejected"
            );
        }
    }

    #[test]
    fn invalid_ids_and_types_fail_closed() {
        let reference = ResourceRef {
            application: ApplicationId::new("io.elembra.files"),
            resource_type: "FILE".into(),
            resource_id: "abc".into(),
            version: None,
        };
        assert!(reference.validate().is_err());

        let reference = ResourceRef {
            application: ApplicationId::new("io.elembra.files"),
            resource_type: "file".into(),
            resource_id: "".into(),
            version: None,
        };
        assert!(reference.validate().is_err());

        let reference = ResourceRef {
            application: ApplicationId::new("io.elembra.files"),
            resource_type: "file".into(),
            resource_id: "a".repeat(MAX_RESOURCE_ID_LEN + 1),
            version: None,
        };
        assert!(reference.validate().is_err());

        let reference = ResourceRef {
            application: ApplicationId::new("io.elembra.files"),
            resource_type: "file".into(),
            resource_id: "has space".into(),
            version: None,
        };
        assert!(reference.validate().is_err());

        let reference = ResourceRef {
            application: ApplicationId::new("io.elembra.files"),
            resource_type: "file".into(),
            resource_id: "abc".into(),
            version: Some("noversionselector".into()),
        };
        assert!(reference.validate().is_err());
    }

    #[test]
    fn invalid_application_namespace_fails_closed() {
        let reference = ResourceRef {
            application: ApplicationId::new("io.elembra.Files"),
            resource_type: "file".into(),
            resource_id: "abc".into(),
            version: None,
        };
        assert!(reference.validate().is_err());
    }

    #[test]
    fn from_str_uses_uri_form() {
        let uri = sample_ref().to_uri();
        let parsed: ResourceRef = uri.parse().unwrap();
        assert_eq!(parsed, sample_ref());
        assert_eq!(sample_ref().to_string(), uri);
    }
}
