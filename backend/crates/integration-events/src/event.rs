//! The CloudEvents 1.0-compatible [`IntegrationEvent`] envelope (ADR-0031).
//!
//! The envelope carries routing/provenance context — `source`, `type`,
//! `subject`, tenant/workspace, actor, correlation/causation and a
//! [`ResourceRef`] — plus Application-owned `data`. It is transport-neutral;
//! the transactional PostgreSQL outbox is the initial transport.
//!
//! Validation rules live in [`IntegrationEvent::validate`] and are enforced
//! by [`IntegrationEventBuilder::build`] and by the outbox store before any
//! row is persisted.

use chrono::{DateTime, Utc};
use rustshare_core::domain::{
    ApplicationId, CausationId, CorrelationId, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_resource_auth::resource_ref::ResourceRef;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Maximum serialized size of the whole envelope, in bytes.
pub const MAX_EVENT_BYTES: usize = 128 * 1024;
/// Maximum serialized size of `data`, in bytes.
pub const MAX_EVENT_DATA_BYTES: usize = 64 * 1024;
/// Maximum length of the `type` attribute.
pub const MAX_EVENT_TYPE_LEN: usize = 128;
/// Maximum length of the `source` attribute.
pub const MAX_SOURCE_LEN: usize = 256;
/// Maximum length of the `subject` attribute.
pub const MAX_SUBJECT_LEN: usize = 1024;
/// Maximum length of the `dataschema` attribute.
pub const MAX_DATASCHEMA_LEN: usize = 512;
/// Maximum length of string-valued Elembra extension attributes
/// (`elembraCorrelation`, `elembraCausation`).
pub const MAX_EXTENSION_STRING_LEN: usize = 128;

/// Errors raised by [`IntegrationEvent::validate`] and the builder.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EventValidationError {
    #[error("invalid specversion `{0}`; only `1.0` is supported")]
    InvalidSpecVersion(String),
    #[error("invalid event source: {0}")]
    InvalidSource(String),
    #[error("invalid event type: {0}")]
    InvalidEventType(String),
    #[error("invalid subject: {0}")]
    InvalidSubject(String),
    #[error("invalid dataschema: {0}")]
    InvalidDataschema(String),
    /// The current platform invariant is `WorkspaceId == TenantId` (one
    /// workspace per tenant). We fail closed until real workspace membership
    /// exists — the same policy as #211 for resource references.
    #[error("tenant and workspace must be identical while one workspace maps to one tenant")]
    TenantWorkspaceMismatch,
    #[error("invalid actor: {0}")]
    InvalidActor(String),
    #[error("invalid resource: {0}")]
    InvalidResource(String),
    #[error("event data is {0} bytes, exceeding the {MAX_EVENT_DATA_BYTES} byte limit")]
    DataTooLarge(usize),
    #[error("serialized event is {0} bytes, exceeding the {MAX_EVENT_BYTES} byte limit")]
    EventTooLarge(usize),
    #[error("field `{0}` exceeds maximum length {1}")]
    FieldTooLong(&'static str, usize),
    #[error("subject must equal the resource URI when both are present")]
    ResourceSubjectMismatch,
    #[error("missing required field `{0}`")]
    MissingField(&'static str),
    #[error("invalid extension attribute `{0}`: {1}")]
    InvalidExtensionAttribute(&'static str, String),
}

/// The business Principal or service whose action caused the event
/// (the `elembraActor` extension attribute).
///
/// Serialized as a plain string on the wire, e.g.
/// `"principal:01J..."`, `"service:io.elembra.files"` or `"agent:01J..."`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorRef {
    /// A human (or device) Principal identified by a UUID.
    Principal(PrincipalId),
    /// An Elembra Application/Connector identified by its application id.
    Service(ApplicationId),
    /// An Agent Principal identified by a UUID.
    Agent(PrincipalId),
}

impl fmt::Display for ActorRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActorRef::Principal(id) => write!(f, "principal:{id}"),
            ActorRef::Service(application) => write!(f, "service:{application}"),
            ActorRef::Agent(id) => write!(f, "agent:{id}"),
        }
    }
}

/// Errors raised while parsing an [`ActorRef`] from its wire string form.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ActorRefError {
    #[error(
        "actor reference must be `principal:<uuid>`, `service:<application-id>` or `agent:<uuid>`"
    )]
    Malformed,
    #[error("invalid principal id in actor reference: {0}")]
    InvalidPrincipal(String),
    #[error("invalid application id in actor reference: {0}")]
    InvalidApplication(String),
}

impl FromStr for ActorRef {
    type Err = ActorRefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (kind, value) = s.split_once(':').ok_or(ActorRefError::Malformed)?;
        if value.is_empty() {
            return Err(ActorRefError::Malformed);
        }
        match kind {
            "principal" => Ok(ActorRef::Principal(PrincipalId(
                Uuid::parse_str(value)
                    .map_err(|e| ActorRefError::InvalidPrincipal(e.to_string()))?,
            ))),
            "service" => {
                if !valid_namespace_rule(value) {
                    return Err(ActorRefError::InvalidApplication(value.to_string()));
                }
                Ok(ActorRef::Service(ApplicationId::new(value)))
            }
            "agent" => Ok(ActorRef::Agent(PrincipalId(
                Uuid::parse_str(value)
                    .map_err(|e| ActorRefError::InvalidPrincipal(e.to_string()))?,
            ))),
            _ => Err(ActorRefError::Malformed),
        }
    }
}

impl Serialize for ActorRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ActorRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        ActorRef::from_str(&raw).map_err(serde::de::Error::custom)
    }
}

fn default_json_content_type() -> String {
    "application/json".to_string()
}

/// A CloudEvents 1.0-compatible envelope for a durable Integration Event.
///
/// Field semantics follow `docs/specs/integration-event-v1alpha1.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntegrationEvent {
    /// CloudEvents spec version; must be `"1.0"`.
    pub specversion: String,
    /// Globally stable event id (`Uuid::new_v4()`); consumers deduplicate on
    /// `(source, id)`.
    pub id: Uuid,
    /// Canonical URI of the publishing Application/Connector, e.g.
    /// `elembra://io.elembra.files`.
    pub source: String,
    /// Namespaced, versioned event type, e.g.
    /// `io.elembra.files.file.created.v1`.
    #[serde(rename = "type")]
    pub r#type: String,
    /// Resource URI in [`ResourceRef`] form when semantically applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// When the event occurred.
    pub time: DateTime<Utc>,
    /// Media type of `data`; defaults to `application/json`.
    #[serde(default = "default_json_content_type")]
    pub datacontenttype: String,
    /// URI of the event schema, e.g.
    /// `https://schemas.elembra.io/events/files/file-updated-v1.json`.
    /// Restricted to `https://` or `elembra:` schemes so schemas are always
    /// resolvable through the Elembra schema registry or HTTPS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataschema: Option<String>,
    /// Tenant scope. Must equal [`IntegrationEvent::workspace_id`] while one
    /// workspace maps to one tenant (platform invariant, see #211).
    #[serde(rename = "elembraTenant")]
    pub tenant_id: TenantId,
    /// Workspace scope. See [`IntegrationEvent::tenant_id`].
    #[serde(rename = "elembraWorkspace")]
    pub workspace_id: WorkspaceId,
    /// The business Principal or service that initiated the event.
    #[serde(rename = "elembraActor", skip_serializing_if = "Option::is_none")]
    pub actor: Option<ActorRef>,
    /// Stable id across one user/workflow operation spanning Applications.
    #[serde(rename = "elembraCorrelation", skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<CorrelationId>,
    /// Event/request id that directly caused this event.
    #[serde(rename = "elembraCausation", skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<CausationId>,
    /// Opaque reference to the Application-owned resource this event is about
    /// (ADR-0032). Its presence grants no access.
    #[serde(rename = "elembraResource", skip_serializing_if = "Option::is_none")]
    pub resource: Option<ResourceRef>,
    /// Application-owned payload. Keep it minimal: prefer
    /// `elembraResource` + provenance over copying source content.
    pub data: serde_json::Value,
}

impl IntegrationEvent {
    /// Start building an event with defaults (`specversion` `"1.0"`, fresh
    /// `id`, `time` now, `datacontenttype` `application/json`, empty `data`).
    pub fn builder() -> IntegrationEventBuilder {
        IntegrationEventBuilder::default()
    }

    /// Validate the full envelope against the v1alpha1 rules.
    ///
    /// Fail-closed checks:
    /// * `specversion` is `1.0`;
    /// * `source` is `elembra://<application-id>` and within
    ///   [`MAX_SOURCE_LEN`];
    /// * `type` matches the event-type syntax, is within
    ///   [`MAX_EVENT_TYPE_LEN`], and its application segment (2nd dotted
    ///   segment) equals the source application's last segment — a publisher
    ///   may only emit events from its own application namespace;
    /// * `subject`, when present, parses as a [`ResourceRef`], is within
    ///   [`MAX_SUBJECT_LEN`] and equals `resource.to_uri()` when a resource
    ///   is also present;
    /// * `dataschema`, when present, is non-empty, within
    ///   [`MAX_DATASCHEMA_LEN`] and starts with `https://` or `elembra:`;
    /// * `tenant_id == workspace_id` (platform invariant, fail closed);
    /// * `actor`, when present, parses via [`ActorRef::from_str`];
    /// * `elembraCorrelation`/`elembraCausation`, when present, are non-empty
    ///   and within [`MAX_EXTENSION_STRING_LEN`];
    /// * `resource.validate()` passes;
    /// * serialized `data` ≤ [`MAX_EVENT_DATA_BYTES`] and the serialized
    ///   envelope ≤ [`MAX_EVENT_BYTES`].
    pub fn validate(&self) -> Result<(), EventValidationError> {
        if self.specversion != "1.0" {
            return Err(EventValidationError::InvalidSpecVersion(
                self.specversion.clone(),
            ));
        }
        if self.source.len() > MAX_SOURCE_LEN {
            return Err(EventValidationError::FieldTooLong("source", MAX_SOURCE_LEN));
        }
        validate_source_uri(&self.source)?;
        if self.r#type.len() > MAX_EVENT_TYPE_LEN {
            return Err(EventValidationError::FieldTooLong(
                "type",
                MAX_EVENT_TYPE_LEN,
            ));
        }
        validate_event_type(&self.r#type)?;

        // Publisher identity consistency: the event type's application
        // segment (`files` in `io.elembra.files.file.created.v1`) must equal
        // the source application's last segment (`files` in
        // `elembra://io.elembra.files`).
        let source_application = validate_source_uri(&self.source)?;
        let type_application_segment = self.r#type.split('.').nth(2).unwrap_or_default();
        let source_application_segment = source_application.rsplit('.').next().unwrap_or_default();
        if type_application_segment != source_application_segment {
            return Err(EventValidationError::InvalidEventType(format!(
                "event type `{}` does not belong to source application `{}`",
                self.r#type, source_application
            )));
        }

        if let Some(subject) = &self.subject {
            if subject.len() > MAX_SUBJECT_LEN {
                return Err(EventValidationError::FieldTooLong(
                    "subject",
                    MAX_SUBJECT_LEN,
                ));
            }
            ResourceRef::from_str(subject)
                .map_err(|e| EventValidationError::InvalidSubject(e.to_string()))?;
            if let Some(resource) = &self.resource {
                if subject != &resource.to_uri() {
                    return Err(EventValidationError::ResourceSubjectMismatch);
                }
            }
        }

        if let Some(dataschema) = &self.dataschema {
            if dataschema.is_empty() || dataschema.len() > MAX_DATASCHEMA_LEN {
                return Err(EventValidationError::InvalidDataschema(format!(
                    "dataschema `{dataschema}` must be non-empty and at most {MAX_DATASCHEMA_LEN} bytes"
                )));
            }
            if !(dataschema.starts_with("https://") || dataschema.starts_with("elembra:")) {
                return Err(EventValidationError::InvalidDataschema(format!(
                    "dataschema `{dataschema}` must start with `https://` or `elembra:`"
                )));
            }
        }

        if self.tenant_id.0 != self.workspace_id.0 {
            return Err(EventValidationError::TenantWorkspaceMismatch);
        }

        if let Some(actor) = &self.actor {
            // Defensive: the wire form must round-trip (Deserialize already
            // parses it, but re-parse so a Display/FromStr drift is caught).
            ActorRef::from_str(&actor.to_string())
                .map_err(|e| EventValidationError::InvalidActor(e.to_string()))?;
        }

        if let Some(correlation_id) = &self.correlation_id {
            if correlation_id.0.is_empty() || correlation_id.0.len() > MAX_EXTENSION_STRING_LEN {
                return Err(EventValidationError::InvalidExtensionAttribute(
                    "elembraCorrelation",
                    correlation_id.0.clone(),
                ));
            }
        }
        if let Some(causation_id) = &self.causation_id {
            if causation_id.0.is_empty() || causation_id.0.len() > MAX_EXTENSION_STRING_LEN {
                return Err(EventValidationError::InvalidExtensionAttribute(
                    "elembraCausation",
                    causation_id.0.clone(),
                ));
            }
        }

        if let Some(resource) = &self.resource {
            resource
                .validate()
                .map_err(EventValidationError::InvalidResource)?;
        }

        let data_len = serialized_len(&self.data);
        if data_len > MAX_EVENT_DATA_BYTES {
            return Err(EventValidationError::DataTooLarge(data_len));
        }
        let total_len = serialized_len(self);
        if total_len > MAX_EVENT_BYTES {
            return Err(EventValidationError::EventTooLarge(total_len));
        }
        Ok(())
    }

    /// Parse the event's source application id (the namespace after
    /// `elembra://`).
    pub fn source_application(&self) -> Result<ApplicationId, EventValidationError> {
        Ok(ApplicationId::new(validate_source_uri(&self.source)?))
    }

    /// The `subject` parsed as a [`ResourceRef`], if present and parseable.
    pub fn subject_resource(&self) -> Option<ResourceRef> {
        self.subject
            .as_deref()
            .and_then(|s| ResourceRef::from_str(s).ok())
    }
}

/// Serialize `value` and return its byte length; a serialization failure is
/// treated as infinitely large so validation fails closed.
fn serialized_len<T: Serialize>(value: &T) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

/// Builder for [`IntegrationEvent`]; see [`IntegrationEvent::builder`].
#[derive(Debug, Clone)]
pub struct IntegrationEventBuilder {
    specversion: String,
    id: Uuid,
    source: Option<String>,
    r#type: Option<String>,
    subject: Option<String>,
    time: DateTime<Utc>,
    datacontenttype: String,
    dataschema: Option<String>,
    tenant_id: Option<TenantId>,
    workspace_id: Option<WorkspaceId>,
    actor: Option<ActorRef>,
    correlation_id: Option<CorrelationId>,
    causation_id: Option<CausationId>,
    resource: Option<ResourceRef>,
    data: serde_json::Value,
}

impl Default for IntegrationEventBuilder {
    fn default() -> Self {
        Self {
            specversion: "1.0".to_string(),
            id: Uuid::new_v4(),
            source: None,
            r#type: None,
            subject: None,
            time: Utc::now(),
            datacontenttype: "application/json".to_string(),
            dataschema: None,
            tenant_id: None,
            workspace_id: None,
            actor: None,
            correlation_id: None,
            causation_id: None,
            resource: None,
            data: serde_json::Value::Null,
        }
    }
}

impl IntegrationEventBuilder {
    /// Canonical publisher URI, e.g. `elembra://io.elembra.files`.
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Namespaced event type, e.g. `io.elembra.files.file.created.v1`.
    pub fn r#type(mut self, r#type: impl Into<String>) -> Self {
        self.r#type = Some(r#type.into());
        self
    }

    /// Resource URI in [`ResourceRef`] form.
    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Event schema URI (`https://` or `elembra:` only).
    pub fn dataschema(mut self, dataschema: impl Into<String>) -> Self {
        self.dataschema = Some(dataschema.into());
        self
    }

    /// Tenant scope; must equal `workspace_id` (platform invariant).
    pub fn tenant_id(mut self, tenant_id: TenantId) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Workspace scope; must equal `tenant_id` (platform invariant).
    pub fn workspace_id(mut self, workspace_id: WorkspaceId) -> Self {
        self.workspace_id = Some(workspace_id);
        self
    }

    /// The business Principal or service that initiated the event.
    pub fn actor(mut self, actor: ActorRef) -> Self {
        self.actor = Some(actor);
        self
    }

    /// Stable id across one user/workflow operation spanning Applications.
    pub fn correlation_id(mut self, correlation_id: CorrelationId) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Event/request id that directly caused this event.
    pub fn causation_id(mut self, causation_id: CausationId) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    /// Opaque reference to the Application-owned resource this event is about.
    pub fn resource(mut self, resource: ResourceRef) -> Self {
        self.resource = Some(resource);
        self
    }

    /// Application-owned payload.
    pub fn data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }

    /// Build the event, validating the full envelope.
    pub fn build(self) -> Result<IntegrationEvent, EventValidationError> {
        let event = IntegrationEvent {
            specversion: self.specversion,
            id: self.id,
            source: self
                .source
                .ok_or(EventValidationError::MissingField("source"))?,
            r#type: self
                .r#type
                .ok_or(EventValidationError::MissingField("type"))?,
            subject: self.subject,
            time: self.time,
            datacontenttype: self.datacontenttype,
            dataschema: self.dataschema,
            tenant_id: self
                .tenant_id
                .ok_or(EventValidationError::MissingField("tenant_id"))?,
            workspace_id: self
                .workspace_id
                .ok_or(EventValidationError::MissingField("workspace_id"))?,
            actor: self.actor,
            correlation_id: self.correlation_id,
            causation_id: self.causation_id,
            resource: self.resource,
            data: self.data,
        };
        event.validate()?;
        Ok(event)
    }
}

/// Validate the event-type syntax `io.elembra.<domain>...<event>.v<N>`.
///
/// Segments are ASCII lowercase letters, digits, `-` and `_`; there must be
/// at least one domain segment between `io.elembra.` and the trailing
/// `.v<N>` with `N >= 1`. The same rule applies to Application manifest
/// declarations in `rustshare-core`.
pub fn validate_event_type(event_type: &str) -> Result<(), EventValidationError> {
    if !valid_event_type_rule(event_type) {
        return Err(EventValidationError::InvalidEventType(
            event_type.to_string(),
        ));
    }
    Ok(())
}

/// Validate a canonical event source URI `elembra://<application-id>` and
/// return the application id string (the namespace after `elembra://`).
pub fn validate_source_uri(source: &str) -> Result<String, EventValidationError> {
    let namespace = source.strip_prefix("elembra://").ok_or_else(|| {
        EventValidationError::InvalidSource("missing `elembra://` scheme prefix".to_string())
    })?;
    if namespace.is_empty() || namespace.contains('/') || !valid_namespace_rule(namespace) {
        return Err(EventValidationError::InvalidSource(format!(
            "`{source}` is not a valid `elembra://<application-id>` source URI"
        )));
    }
    Ok(namespace.to_string())
}

/// Namespace syntax shared with `rustshare-core::domain::valid_namespace`:
/// one or more dot-separated segments of ASCII lowercase letters, digits,
/// `-` and `_`.
fn valid_namespace_rule(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        })
}

/// The event-type rule: `io.elembra.` + one or more namespace segments + a
/// trailing `.v<N>` with `N >= 1` (the version part must not be zero).
fn valid_event_type_rule(event_type: &str) -> bool {
    let Some(rest) = event_type.strip_prefix("io.elembra.") else {
        return false;
    };
    let Some(version_at) = rest.rfind(".v") else {
        return false;
    };
    let version = &rest[version_at + 2..];
    if version.is_empty() || !version.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if version.trim_start_matches('0').is_empty() {
        return false; // `.v0` / `.v00` — major version must be >= 1
    }
    let domain = &rest[..version_at];
    !domain.is_empty() && valid_namespace_rule(domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_resource() -> ResourceRef {
        ResourceRef::new(ApplicationId::new("io.elembra.files"), "file", "01K2ABC123")
            .with_version("sha256:0123abcdef")
    }

    fn sample_event() -> IntegrationEvent {
        let tenant = TenantId(Uuid::new_v4());
        let resource = sample_resource();
        IntegrationEvent::builder()
            .source("elembra://io.elembra.files")
            .r#type("io.elembra.files.file.created.v1")
            .subject(resource.to_uri())
            .tenant_id(tenant)
            .workspace_id(WorkspaceId(tenant.0))
            .actor(ActorRef::Principal(PrincipalId(Uuid::new_v4())))
            .resource(resource)
            .data(json!({"name": "architecture.md", "mime_type": "text/markdown", "size": 12420}))
            .build()
            .unwrap()
    }

    #[test]
    fn builder_round_trips_through_json() {
        let event = sample_event();
        let json = serde_json::to_string(&event).unwrap();
        let parsed: IntegrationEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event);
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["specversion"], "1.0");
        assert_eq!(value["source"], "elembra://io.elembra.files");
        assert_eq!(value["type"], "io.elembra.files.file.created.v1");
        assert_eq!(value["datacontenttype"], "application/json");
        assert!(value.get("elembraTenant").is_some());
        assert!(value.get("elembraWorkspace").is_some());
        assert_eq!(
            value["elembraActor"]
                .as_str()
                .unwrap()
                .starts_with("principal:"),
            true
        );
        assert_eq!(value["elembraResource"]["resourceType"], "file");
        assert_eq!(value["elembraResource"]["version"], "sha256:0123abcdef");
        assert!(value.get("elembraCorrelation").is_none());
    }

    #[test]
    fn parses_and_validates_the_spec_example_envelope() {
        let tenant = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let principal = Uuid::new_v4();
        let subject =
            format!("elembra://io.elembra.files/file/{file_id}?version=sha256%3A0123abcdef");
        let json = serde_json::json!({
            "specversion": "1.0",
            "id": Uuid::new_v4().to_string(),
            "source": "elembra://io.elembra.files",
            "type": "io.elembra.files.file.updated.v1",
            "subject": subject,
            "time": "2026-08-07T19:00:00Z",
            "datacontenttype": "application/json",
            "dataschema": "https://schemas.elembra.io/events/files/file-updated-v1.json",
            "elembraTenant": tenant.to_string(),
            "elembraWorkspace": tenant.to_string(),
            "elembraActor": format!("principal:{principal}"),
            "elembraCorrelation": "01K2ABCDEFGHIJKLMNOPQRSTUV",
            "elembraCausation": "01K2ABCDEFGHIJKLMNOPQRSTUV",
            "elembraResource": {
                "application": "io.elembra.files",
                "resourceType": "file",
                "resourceId": file_id.to_string(),
                "version": "sha256:0123abcdef"
            },
            "data": {
                "name": "architecture.md",
                "mime_type": "text/markdown",
                "size": 12420
            }
        });
        let event: IntegrationEvent = serde_json::from_value(json).unwrap();
        event.validate().unwrap();
    }

    #[test]
    fn rejects_wrong_specversion() {
        let mut event = sample_event();
        event.specversion = "0.3".into();
        assert_eq!(
            event.validate(),
            Err(EventValidationError::InvalidSpecVersion("0.3".into()))
        );
    }

    #[test]
    fn rejects_type_not_owned_by_source_application() {
        let mut event = sample_event();
        event.r#type = "io.elembra.mail.message.archived.v1".into();
        assert!(matches!(
            event.validate(),
            Err(EventValidationError::InvalidEventType(_))
        ));
    }

    #[test]
    fn rejects_tenant_workspace_mismatch() {
        let mut event = sample_event();
        event.workspace_id = WorkspaceId(Uuid::new_v4());
        assert_eq!(
            event.validate(),
            Err(EventValidationError::TenantWorkspaceMismatch)
        );
    }

    #[test]
    fn rejects_subject_resource_mismatch() {
        let mut event = sample_event();
        event.subject = Some(
            ResourceRef::new(ApplicationId::new("io.elembra.files"), "file", "other-id").to_uri(),
        );
        assert_eq!(
            event.validate(),
            Err(EventValidationError::ResourceSubjectMismatch)
        );
    }

    #[test]
    fn rejects_invalid_dataschema() {
        let mut event = sample_event();
        event.dataschema = Some("ftp://schemas.example.com/event.json".into());
        assert!(matches!(
            event.validate(),
            Err(EventValidationError::InvalidDataschema(_))
        ));
        let mut event = sample_event();
        event.dataschema = Some("elembra:events/files/file-updated-v1.json".into());
        assert!(event.validate().is_ok());
    }

    #[test]
    fn rejects_empty_and_oversized_correlation() {
        let mut event = sample_event();
        event.correlation_id = Some(CorrelationId::new(""));
        assert!(matches!(
            event.validate(),
            Err(EventValidationError::InvalidExtensionAttribute(
                "elembraCorrelation",
                _
            ))
        ));
        let mut event = sample_event();
        event.causation_id = Some(CausationId::new("x".repeat(MAX_EXTENSION_STRING_LEN + 1)));
        assert!(matches!(
            event.validate(),
            Err(EventValidationError::InvalidExtensionAttribute(
                "elembraCausation",
                _
            ))
        ));
    }

    #[test]
    fn rejects_oversized_data() {
        let mut event = sample_event();
        event.data = json!({"blob": "x".repeat(MAX_EVENT_DATA_BYTES + 1)});
        assert!(matches!(
            event.validate(),
            Err(EventValidationError::DataTooLarge(_))
        ));
        // Just under the limit passes.
        let mut event = sample_event();
        event.data = json!({"blob": "x".repeat(MAX_EVENT_DATA_BYTES - 64)});
        assert!(event.validate().is_ok());
    }

    #[test]
    fn rejects_missing_required_builder_fields() {
        let err = IntegrationEvent::builder().build().unwrap_err();
        assert!(matches!(err, EventValidationError::MissingField("source")));
        let err = IntegrationEvent::builder()
            .source("elembra://io.elembra.files")
            .build()
            .unwrap_err();
        assert!(matches!(err, EventValidationError::MissingField("type")));
        let err = IntegrationEvent::builder()
            .source("elembra://io.elembra.files")
            .r#type("io.elembra.files.file.created.v1")
            .build()
            .unwrap_err();
        assert!(matches!(
            err,
            EventValidationError::MissingField("tenant_id")
        ));
    }

    #[test]
    fn validates_event_type_syntax() {
        for valid in [
            "io.elembra.files.file.created.v1",
            "io.elembra.files.file.updated.v1",
            "io.elembra.files.v1",
            "io.elembra.files.file.created.v01",
            "io.elembra.mail.message.archived.v1",
            "io.elembra.connector.shell.command.captured.v1",
            "io.elembra.files.file-sync.failed.v10",
        ] {
            assert!(
                validate_event_type(valid).is_ok(),
                "{valid} should be valid"
            );
        }
        for invalid in [
            "garbage",
            "",
            "io.elembra",
            "io.elembra.files",
            "io.elembra.files.file.created",
            "io.elembra.files.file.created.v",
            "io.elembra.files.file.created.v0",
            "io.elembra.files.file.created.v00",
            "io.elembra.files.file.created.V1",
            "io.elembra.files.file.created.v1x",
            "io.elembra..file.created.v1",
            "io.elembra.files.file.created.v1.extra",
            "com.example.foo.bar.v1",
            "io.elembra.files.file.created.v1!",
        ] {
            assert!(
                validate_event_type(invalid).is_err(),
                "{invalid} should be invalid"
            );
        }
    }

    #[test]
    fn validates_source_uri_syntax() {
        assert_eq!(
            validate_source_uri("elembra://io.elembra.files").unwrap(),
            "io.elembra.files"
        );
        for invalid in [
            "https://io.elembra.files",
            "elembra://",
            "elembra://io.elembra.Files",
            "elembra://io.elembra.files/extra",
            "elembra://io elembra",
            "io.elembra.files",
        ] {
            assert!(
                validate_source_uri(invalid).is_err(),
                "{invalid} should be invalid"
            );
        }
    }

    #[test]
    fn actor_ref_display_parse_and_serde_round_trip() {
        let principal = ActorRef::Principal(PrincipalId(Uuid::new_v4()));
        assert_eq!(
            ActorRef::from_str(&principal.to_string()).unwrap(),
            principal
        );
        let service = ActorRef::Service(ApplicationId::new("io.elembra.files"));
        assert_eq!(ActorRef::from_str(&service.to_string()).unwrap(), service);
        let agent = ActorRef::Agent(PrincipalId(Uuid::new_v4()));
        assert_eq!(ActorRef::from_str(&agent.to_string()).unwrap(), agent);

        let json = serde_json::to_string(&principal).unwrap();
        assert_eq!(json, format!("\"{}\"", principal.to_string()));
        let parsed: ActorRef = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, principal);

        for invalid in [
            "",
            "principal:not-a-uuid",
            "principal:",
            "service:",
            "service:io.elembra.Files",
            "user:01JABCDEFGHIJKLMNOPQRSTUV",
            "principal:01JABCDEFGHIJKLMNOPQRSTUV:extra",
        ] {
            assert!(
                ActorRef::from_str(invalid).is_err(),
                "{invalid} should be rejected"
            );
        }
        let invalid: Result<ActorRef, _> = serde_json::from_str("\"principal:not-a-uuid\"");
        assert!(invalid.is_err());
    }

    #[test]
    fn source_application_and_subject_resource_helpers() {
        let event = sample_event();
        assert_eq!(
            event.source_application().unwrap(),
            ApplicationId::new("io.elembra.files")
        );
        assert_eq!(event.subject_resource().unwrap(), sample_resource());
        let mut event = sample_event();
        event.subject = None;
        assert_eq!(event.subject_resource(), None);
    }

    #[test]
    fn oversized_source_and_type_are_rejected() {
        let mut event = sample_event();
        event.source = format!("elembra://io.elembra.{}", "a".repeat(MAX_SOURCE_LEN));
        assert!(matches!(
            event.validate(),
            Err(EventValidationError::FieldTooLong("source", _))
        ));
        let mut event = sample_event();
        event.r#type = format!("io.elembra.files.{}.v1", "x".repeat(MAX_EVENT_TYPE_LEN));
        assert!(matches!(
            event.validate(),
            Err(EventValidationError::FieldTooLong("type", _))
        ));
    }
}
