//! Credential-free Elembra Files references carried by Buzz events.

use nostr::Tag;
use serde::{Deserialize, Serialize};

use crate::ResourceRef;

/// Generic Nostr tag used by Chat messages for an Elembra resource reference.
/// The value is an opaque canonical `ResourceRef` URI; it is never authority.
pub const BUZZ_RESOURCE_REF_TAG: &str = "elembra-ref";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChatResourceError {
    #[error("message must contain exactly one `{BUZZ_RESOURCE_REF_TAG}` tag")]
    InvalidTagCount,
    #[error("invalid `{BUZZ_RESOURCE_REF_TAG}` tag: {0}")]
    InvalidReference(String),
}

/// The only attachment data returned to a composer before the user signs a
/// normal Buzz event. Display fields are safe hints; the URI remains the
/// signed, canonical relationship.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatResourceAttachment {
    pub resource: ResourceRef,
    pub display_name: String,
    pub media_type: Option<String>,
    pub size: Option<i64>,
    pub available: bool,
}

/// Build the interoperable Nostr tag for a Chat attachment.
pub fn buzz_resource_ref_tag(resource: &ResourceRef) -> Result<Tag, ChatResourceError> {
    resource
        .validate()
        .map_err(ChatResourceError::InvalidReference)?;
    let uri = resource.to_uri();
    Ok(Tag::parse([BUZZ_RESOURCE_REF_TAG, uri.as_str()]).expect("validated tag"))
}

/// Extract one attachment reference from a signed Buzz event's tags.
/// Duplicate tags are rejected instead of choosing an attacker-controlled
/// interpretation.
pub fn resource_ref_from_tags(tags: &[Tag]) -> Result<Option<ResourceRef>, ChatResourceError> {
    let matches: Vec<&Tag> = tags
        .iter()
        .filter(|tag| tag.as_slice().first().map(String::as_str) == Some(BUZZ_RESOURCE_REF_TAG))
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [tag] => {
            let fields = tag.as_slice();
            if fields.len() != 2 {
                return Err(ChatResourceError::InvalidTagCount);
            }
            fields[1]
                .parse::<ResourceRef>()
                .map(Some)
                .map_err(|error| ChatResourceError::InvalidReference(error.to_string()))
        }
        _ => Err(ChatResourceError::InvalidTagCount),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind};
    use rustshare_core::domain::ApplicationId;

    fn resource() -> ResourceRef {
        ResourceRef::new(ApplicationId::new("io.elembra.files"), "file", "file-1")
            .with_version(format!("sha256:{}", "a".repeat(64)))
    }

    #[test]
    fn tag_round_trips_without_credentials() {
        let tag = buzz_resource_ref_tag(&resource()).unwrap();
        let event = EventBuilder::new(Kind::TextNote, "secret body")
            .tag(tag.clone())
            .sign_with_keys(&Keys::generate())
            .unwrap();
        assert_eq!(
            resource_ref_from_tags(event.tags.as_slice()).unwrap(),
            Some(resource())
        );
        let encoded = serde_json::to_string(&event).unwrap();
        assert!(!encoded.contains("token"));
        assert!(!encoded.contains("presigned"));
        assert!(!encoded.contains("storage_key"));
    }

    #[test]
    fn duplicate_or_malformed_tags_fail_closed() {
        let tag = buzz_resource_ref_tag(&resource()).unwrap();
        assert_eq!(
            resource_ref_from_tags(&[tag.clone(), tag]).unwrap_err(),
            ChatResourceError::InvalidTagCount
        );
        let malformed = Tag::parse([BUZZ_RESOURCE_REF_TAG, "https://evil.invalid/file"]).unwrap();
        assert!(matches!(
            resource_ref_from_tags(&[malformed]),
            Err(ChatResourceError::InvalidReference(_))
        ));
        let uri = resource().to_uri();
        let extra = Tag::parse([BUZZ_RESOURCE_REF_TAG, uri.as_str(), "secret"]).unwrap();
        assert_eq!(
            resource_ref_from_tags(&[extra]).unwrap_err(),
            ChatResourceError::InvalidTagCount
        );
    }

    #[test]
    fn unrelated_standard_tags_are_ignored() {
        let pubkey = "02".repeat(32);
        let p = Tag::parse(["p", pubkey.as_str()]).unwrap();
        assert_eq!(resource_ref_from_tags(&[p]).unwrap(), None);
    }
}
