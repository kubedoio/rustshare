//! Declarative Elembra Application contracts.
//!
//! This module contains identity and manifest metadata only. It deliberately
//! does not load code, resolve services, or implement application business
//! logic.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use uuid::Uuid;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }
        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

string_id!(ApplicationId);
string_id!(ActionCapability);
string_id!(CorrelationId);
string_id!(CausationId);
uuid_id!(TenantId);
uuid_id!(WorkspaceId);
uuid_id!(PrincipalId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ApplicationRuntimeKind {
    Embedded,
    Service,
    Bridge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ContractRef {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApplicationResource {
    #[serde(rename = "type")]
    pub resource_type: String,
    pub actions: Vec<ActionCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct ApplicationContribution {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub route: Option<String>,
    #[serde(default)]
    pub renderer: Option<String>,
    #[serde(default)]
    pub action: Option<ActionCapability>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub order: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct ApplicationContributions {
    #[serde(default)]
    pub navigation: Vec<ApplicationContribution>,
    #[serde(default)]
    pub routes: Vec<ApplicationContribution>,
    #[serde(default)]
    pub commands: Vec<ApplicationContribution>,
    #[serde(default)]
    pub dashboard: Vec<ApplicationContribution>,
    #[serde(default)]
    pub settings: Vec<ApplicationContribution>,
    #[serde(default, rename = "searchProviders")]
    pub search_providers: Vec<ApplicationContribution>,
    #[serde(default)]
    pub renderers: Vec<ApplicationContribution>,
    #[serde(default)]
    pub admin: Vec<ApplicationContribution>,
}

impl ApplicationContributions {
    pub fn all(&self) -> Vec<&ApplicationContribution> {
        [
            &self.navigation,
            &self.routes,
            &self.commands,
            &self.dashboard,
            &self.settings,
            &self.search_providers,
            &self.renderers,
            &self.admin,
        ]
        .into_iter()
        .flat_map(|contributions| contributions.iter())
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApplicationManifest {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: ApplicationMetadata,
    pub runtime: ApplicationRuntime,
    pub contracts: ApplicationContracts,
    #[serde(default)]
    pub resources: Vec<ApplicationResource>,
    #[serde(default)]
    pub contributions: ApplicationContributions,
    #[serde(rename = "integrationEvents", default)]
    pub integration_events: IntegrationEvents,
    #[serde(default)]
    pub memory: Option<MemoryPolicy>,
    pub configuration: ConfigurationReference,
    pub data: DataPolicy,
    #[serde(default)]
    pub health: Option<HealthMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApplicationMetadata {
    pub id: ApplicationId,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApplicationRuntime {
    pub kind: ApplicationRuntimeKind,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct ApplicationContracts {
    #[serde(default)]
    pub provides: Vec<ContractRef>,
    #[serde(default)]
    pub requires: Vec<ContractRef>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct IntegrationEvents {
    #[serde(default)]
    pub publishes: Vec<String>,
    #[serde(default)]
    pub subscribes: Vec<String>,
}

impl ApplicationManifest {
    /// Whether this manifest declares `event_type` in
    /// `integration_events.publishes` (i.e. the Application owns that
    /// integration-event contract).
    pub fn publishes_event(&self, event_type: &str) -> bool {
        self.integration_events
            .publishes
            .iter()
            .any(|declared| declared == event_type)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MemoryPolicy {
    #[serde(rename = "sourceTypes", default)]
    pub source_types: Vec<String>,
    pub publication: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConfigurationReference {
    pub schema: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DataPolicy {
    pub owner: ApplicationId,
    #[serde(rename = "preserveOnDisable")]
    pub preserve_on_disable: bool,
    #[serde(rename = "exportSupported")]
    pub export_supported: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct HealthMetadata {
    pub liveness: String,
    pub readiness: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ApplicationHealth {
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationState {
    Available,
    Configured,
    Enabled,
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationEnablement {
    pub tenant_id: TenantId,
    pub workspace_id: WorkspaceId,
    pub application_id: ApplicationId,
    pub enabled: bool,
    pub configuration: serde_json::Value,
    pub health: ApplicationHealth,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApplicationShellEntry {
    pub manifest: ApplicationManifest,
    pub enabled: bool,
    pub configuration: serde_json::Value,
    pub health: ApplicationHealth,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ApplicationRegistryError {
    #[error("invalid application manifest: {0}")]
    InvalidManifest(String),
    #[error("duplicate application id: {0}")]
    DuplicateApplication(ApplicationId),
    #[error("required contract is not provided: {0}@{1}")]
    MissingContract(String, String),
    #[error("duplicate contribution id: {0}")]
    DuplicateContribution(String),
    #[error("duplicate action capability namespace: {0}")]
    DuplicateActionNamespace(String),
    #[error("invalid integration event type: {0}")]
    InvalidEventType(String),
}

pub struct ApplicationRegistry {
    manifests: HashMap<ApplicationId, ApplicationManifest>,
    enablement: HashMap<(TenantId, WorkspaceId, ApplicationId), ApplicationEnablement>,
}

impl ApplicationRegistry {
    pub fn first_party() -> Result<Self, ApplicationRegistryError> {
        Self::new(first_party_manifests())
    }

    pub fn new(
        manifests: impl IntoIterator<Item = ApplicationManifest>,
    ) -> Result<Self, ApplicationRegistryError> {
        let mut registry = Self {
            manifests: HashMap::new(),
            enablement: HashMap::new(),
        };
        for manifest in manifests {
            registry.register(manifest)?;
        }
        let provided: HashSet<_> = registry
            .manifests
            .values()
            .flat_map(|m| m.contracts.provides.iter().map(|c| (&c.id, &c.version)))
            .collect();
        for manifest in registry.manifests.values() {
            for required in &manifest.contracts.requires {
                if !provided.contains(&(&required.id, &required.version)) {
                    return Err(ApplicationRegistryError::MissingContract(
                        required.id.clone(),
                        required.version.clone(),
                    ));
                }
            }
        }
        let mut action_owners = HashMap::<String, ApplicationId>::new();
        for manifest in registry.manifests.values() {
            for action in manifest
                .resources
                .iter()
                .flat_map(|resource| resource.actions.iter())
                .chain(
                    manifest
                        .contributions
                        .all()
                        .into_iter()
                        .filter_map(|contribution| contribution.action.as_ref()),
                )
            {
                let namespace = action_namespace(action).ok_or_else(|| {
                    ApplicationRegistryError::InvalidManifest(format!(
                        "invalid action capability: {action}"
                    ))
                })?;
                if let Some(owner) =
                    action_owners.insert(namespace.clone(), manifest.metadata.id.clone())
                {
                    if owner != manifest.metadata.id {
                        return Err(ApplicationRegistryError::DuplicateActionNamespace(
                            namespace,
                        ));
                    }
                }
            }
        }
        Ok(registry)
    }

    fn register(&mut self, manifest: ApplicationManifest) -> Result<(), ApplicationRegistryError> {
        validate_manifest(&manifest)?;
        if self.manifests.contains_key(&manifest.metadata.id) {
            return Err(ApplicationRegistryError::DuplicateApplication(
                manifest.metadata.id,
            ));
        }
        self.manifests
            .insert(manifest.metadata.id.clone(), manifest);
        Ok(())
    }

    pub fn available(&self) -> impl Iterator<Item = &ApplicationManifest> {
        self.manifests.values()
    }
    pub fn manifest(&self, id: &ApplicationId) -> Option<&ApplicationManifest> {
        self.manifests.get(id)
    }
    /// Whether `application`'s manifest declares that it publishes
    /// `event_type`.
    pub fn owns_event_type(&self, application: &ApplicationId, event_type: &str) -> bool {
        self.manifest(application)
            .is_some_and(|manifest| manifest.publishes_event(event_type))
    }
    pub fn state(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        application_id: &ApplicationId,
    ) -> Option<ApplicationState> {
        self.manifests.get(application_id)?;
        let enablement = self.enablement(tenant_id, workspace_id, application_id);
        Some(match enablement {
            None => ApplicationState::Available,
            Some(entry) if !entry.enabled => ApplicationState::Configured,
            Some(entry) => match entry.health {
                ApplicationHealth::Healthy => ApplicationState::Healthy,
                ApplicationHealth::Degraded => ApplicationState::Degraded,
                ApplicationHealth::Unavailable => ApplicationState::Unavailable,
            },
        })
    }
    pub fn is_configured(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        application_id: &ApplicationId,
    ) -> bool {
        self.enablement(tenant_id, workspace_id, application_id)
            .is_some()
    }
    pub fn is_enabled(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        application_id: &ApplicationId,
    ) -> bool {
        self.enablement(tenant_id, workspace_id, application_id)
            .is_some_and(|entry| entry.enabled)
    }
    pub fn configure(
        &mut self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        application_id: &ApplicationId,
        enabled: bool,
        configuration: serde_json::Value,
    ) -> Option<&ApplicationEnablement> {
        self.manifests.get(application_id)?;
        let key = (tenant_id, workspace_id, application_id.clone());
        self.enablement.insert(
            key.clone(),
            ApplicationEnablement {
                tenant_id,
                workspace_id,
                application_id: application_id.clone(),
                enabled,
                configuration,
                health: ApplicationHealth::Healthy,
            },
        );
        self.enablement.get(&key)
    }
    pub fn enablement(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        application_id: &ApplicationId,
    ) -> Option<&ApplicationEnablement> {
        self.enablement
            .get(&(tenant_id, workspace_id, application_id.clone()))
    }
    pub fn set_health(
        &mut self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
        application_id: &ApplicationId,
        health: ApplicationHealth,
    ) -> bool {
        self.enablement
            .get_mut(&(tenant_id, workspace_id, application_id.clone()))
            .map(|entry| entry.health = health)
            .is_some()
    }
    pub fn contributions(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
    ) -> Vec<&ApplicationContribution> {
        self.enablement
            .values()
            .filter(|e| e.tenant_id == tenant_id && e.workspace_id == workspace_id && e.enabled)
            .filter_map(|e| self.manifests.get(&e.application_id))
            .flat_map(|m| {
                [
                    &m.contributions.navigation,
                    &m.contributions.routes,
                    &m.contributions.commands,
                    &m.contributions.dashboard,
                    &m.contributions.settings,
                    &m.contributions.search_providers,
                    &m.contributions.renderers,
                    &m.contributions.admin,
                ]
            })
            .flat_map(|v| v.iter())
            .collect()
    }
}

/// Returns the code-owned first-party Application contracts.
///
/// This is the only first-party identity/contribution catalogue. Tenant
/// configuration is persisted separately and is never allowed to redefine
/// an Application identity or its Shell contributions.
pub fn first_party_manifests() -> Vec<ApplicationManifest> {
    [
        ("files", "Files", "folder", "files", 10),
        ("notes", "Notes", "sticky-note", "okf-note", 20),
        ("mail", "Mail", "mail", "mail", 30),
        ("meetings", "Meeting Notes", "calendar-days", "meetings", 70),
        ("standups", "Standups", "activity", "standups", 80),
        ("kanban", "Kanban", "columns", "kanban", 90),
        ("decisions", "Decisions", "git-branch", "decisions", 100),
        (
            "brainstorming",
            "Brainstorming",
            "lightbulb",
            "brainstorming",
            110,
        ),
        ("shares", "Shares", "share-2", "shares", 120),
    ]
    .into_iter()
    .map(|(slug, name, icon, renderer, order)| {
        let id = ApplicationId::new(format!("io.elembra.{slug}"));
        let dashboard_action = match slug {
            "notes" | "meetings" | "standups" | "kanban" | "decisions" | "brainstorming" => {
                Some(ActionCapability::new(format!("{slug}.create")))
            }
            _ => None,
        };
        let dashboard_template = match slug {
            "notes" => Some("template_default_okf_note"),
            "meetings" => Some("template_default_meeting"),
            "standups" => Some("template_default_standup"),
            "kanban" => Some("template_default_kanban"),
            "decisions" => Some("template_default_decision"),
            "brainstorming" => Some("template_blank_brainstorm"),
            _ => None,
        };
        // The Files Application owns the `file` and `folder` resource types
        // and declares its full action capability set (ADR-0032). Other
        // Applications keep the generic `{slug}.resource` read declaration
        // until their own resource types are introduced.
        let resources = if slug == "files" {
            vec![
                ApplicationResource {
                    resource_type: "file".into(),
                    actions: vec![
                        ActionCapability::new("files.read"),
                        ActionCapability::new("files.write"),
                        ActionCapability::new("files.delete"),
                        ActionCapability::new("files.share"),
                    ],
                },
                ApplicationResource {
                    resource_type: "folder".into(),
                    actions: vec![
                        ActionCapability::new("files.read"),
                        ActionCapability::new("files.write"),
                        ActionCapability::new("files.delete"),
                        ActionCapability::new("files.share"),
                    ],
                },
            ]
        } else {
            vec![ApplicationResource {
                resource_type: format!("{slug}.resource"),
                actions: vec![ActionCapability::new(format!("{slug}.read"))],
            }]
        };
        // The Files Application owns the durable integration-event contracts
        // for file creation and content updates (ADR-0031). Other
        // Applications declare no integration events yet.
        let integration_events = if slug == "files" {
            IntegrationEvents {
                publishes: vec![
                    "io.elembra.files.file.created.v1".into(),
                    "io.elembra.files.file.updated.v1".into(),
                ],
                subscribes: Vec::new(),
            }
        } else {
            IntegrationEvents::default()
        };
        ApplicationManifest {
            api_version: "elembra.io/v1alpha1".into(),
            kind: "Application".into(),
            metadata: ApplicationMetadata {
                id: id.clone(),
                name: name.into(),
                version: "1.0.0".into(),
                description: format!("Elembra {name} Application"),
            },
            runtime: ApplicationRuntime {
                kind: ApplicationRuntimeKind::Embedded,
            },
            contracts: ApplicationContracts {
                provides: vec![ContractRef {
                    id: format!("{id}.api"),
                    version: "v1alpha1".into(),
                }],
                requires: Vec::new(),
            },
            resources,
            contributions: ApplicationContributions {
                navigation: vec![ApplicationContribution {
                    id: format!("{slug}.navigation"),
                    label: Some(name.into()),
                    icon: Some(icon.into()),
                    route: Some(format!("/apps/{slug}")),
                    order: Some(order),
                    ..Default::default()
                }],
                routes: vec![ApplicationContribution {
                    id: format!("{slug}.route"),
                    route: Some(format!("/apps/{slug}")),
                    renderer: Some(renderer.into()),
                    ..Default::default()
                }],
                dashboard: vec![ApplicationContribution {
                    id: format!("{slug}.dashboard"),
                    label: Some(name.into()),
                    renderer: Some(format!("{renderer}-dashboard")),
                    order: Some(order),
                    action: dashboard_action,
                    template: dashboard_template.map(str::to_string),
                    ..Default::default()
                }],
                settings: vec![ApplicationContribution {
                    id: format!("{slug}.settings"),
                    route: Some(format!("/settings/apps/{slug}")),
                    ..Default::default()
                }],
                ..Default::default()
            },
            integration_events,
            memory: None,
            configuration: ConfigurationReference {
                schema: format!("contracts/{id}/config-v1alpha1.schema.json"),
            },
            data: DataPolicy {
                owner: id,
                preserve_on_disable: true,
                export_supported: true,
            },
            health: None,
        }
    })
    .collect()
}

pub fn validate_manifest(manifest: &ApplicationManifest) -> Result<(), ApplicationRegistryError> {
    if manifest.api_version != "elembra.io/v1alpha1"
        || manifest.kind != "Application"
        || !valid_namespace(&manifest.metadata.id.0)
        || manifest.metadata.name.trim().is_empty()
        || manifest.metadata.version.trim().is_empty()
        || manifest.data.owner != manifest.metadata.id
        || manifest.configuration.schema.trim().is_empty()
    {
        return Err(ApplicationRegistryError::InvalidManifest(
            "invalid identity or ownership".into(),
        ));
    }
    let mut ids = HashSet::new();
    let families = [
        &manifest.contributions.navigation,
        &manifest.contributions.routes,
        &manifest.contributions.commands,
        &manifest.contributions.dashboard,
        &manifest.contributions.settings,
        &manifest.contributions.search_providers,
        &manifest.contributions.renderers,
        &manifest.contributions.admin,
    ];
    let application_namespace = manifest
        .metadata
        .id
        .0
        .rsplit('.')
        .next()
        .unwrap_or_default();
    for contribution in families.into_iter().flat_map(|v| v.iter()) {
        if contribution.id.trim().is_empty() || !ids.insert(&contribution.id) {
            return Err(ApplicationRegistryError::DuplicateContribution(
                contribution.id.clone(),
            ));
        }
        if !valid_namespace(&contribution.id)
            || (contribution.id != application_namespace
                && !contribution
                    .id
                    .starts_with(&format!("{application_namespace}.")))
        {
            return Err(ApplicationRegistryError::InvalidManifest(format!(
                "contribution {} is outside {} namespace",
                contribution.id, application_namespace
            )));
        }
    }
    if manifest
        .resources
        .iter()
        .any(|resource| resource.resource_type.trim().is_empty())
        || manifest
            .contracts
            .provides
            .iter()
            .chain(manifest.contracts.requires.iter())
            .any(|contract| contract.id.trim().is_empty() || contract.version.trim().is_empty())
        || manifest
            .integration_events
            .publishes
            .iter()
            .chain(manifest.integration_events.subscribes.iter())
            .any(|event| event.trim().is_empty())
    {
        return Err(ApplicationRegistryError::InvalidManifest(
            "empty contract, resource, or event declaration".into(),
        ));
    }
    // Integration-event declarations must use the canonical namespaced,
    // versioned syntax (same rule as the envelope's `validate_event_type`).
    for event in manifest
        .integration_events
        .publishes
        .iter()
        .chain(manifest.integration_events.subscribes.iter())
    {
        if !valid_event_type(event) {
            return Err(ApplicationRegistryError::InvalidEventType(event.clone()));
        }
    }
    if let Some(memory) = &manifest.memory {
        if !matches!(
            memory.publication.as_str(),
            "none" | "reference-first" | "artifact-backed"
        ) {
            return Err(ApplicationRegistryError::InvalidManifest(
                "invalid memory publication policy".into(),
            ));
        }
    }
    for action in manifest
        .resources
        .iter()
        .flat_map(|r| r.actions.iter())
        .chain(
            manifest
                .contributions
                .all()
                .into_iter()
                .filter_map(|c| c.action.as_ref()),
        )
    {
        let namespace = action_namespace(action).ok_or_else(|| {
            ApplicationRegistryError::InvalidManifest(format!(
                "invalid action capability: {action}"
            ))
        })?;
        if namespace != application_namespace {
            return Err(ApplicationRegistryError::InvalidManifest(format!(
                "action namespace {namespace} is not owned by {}",
                manifest.metadata.id
            )));
        }
    }
    Ok(())
}

/// Namespace syntax shared by Application IDs, resource types and
/// contribution ids: one or more dot-separated segments of ASCII lowercase
/// letters, digits, `-` and `_`.
pub fn valid_namespace(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        })
}

/// Whether `t` is a well-formed integration-event type:
/// `io.elembra.<domain>...<event>.v<N>` with ASCII lowercase segments and a
/// trailing major version `N >= 1`.
///
/// The version segment is the LAST dot-segment and must be `v` + ASCII
/// digits; no other segment may look like a version (a bare `v`, or `v` +
/// digits), so a version-like segment mid-string (e.g.
/// `io.elembra.files.v1.v2` or `io.elembra.files.v.file.created.v1`) cannot
/// pass envelope syntax.
///
/// This is the same rule the envelope validator
/// (`rustshare-integration-events::validate_event_type`) enforces on event
/// instances; manifests declaring publishes/subscribes must use it too.
pub fn valid_event_type(t: &str) -> bool {
    let Some(rest) = t.strip_prefix("io.elembra.") else {
        return false;
    };
    let Some((domain, version)) = rest.rsplit_once('.') else {
        return false;
    };
    let Some(digits) = version.strip_prefix('v') else {
        return false;
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if digits.trim_start_matches('0').is_empty() {
        return false; // `.v0` / `.v00` — major version must be >= 1
    }
    if domain.is_empty() || !valid_namespace(domain) {
        return false;
    }
    // The version is the last dot-segment and the only one: reject any other
    // version-like segment in the domain. A bare `v` counts as version-like
    // too, so `io.elembra.v.file.created.v1` cannot pass envelope syntax.
    domain.split('.').all(|segment| {
        segment
            .strip_prefix('v')
            .is_none_or(|d| !d.is_empty() && !d.bytes().all(|b| b.is_ascii_digit()))
    })
}

fn action_namespace(action: &ActionCapability) -> Option<String> {
    let mut parts = action.0.split('.');
    let namespace = parts.next()?;
    let verb = parts.next()?;
    if namespace.is_empty() || verb.is_empty() || parts.next().is_some() {
        return None;
    }
    if !valid_namespace(namespace) || !valid_namespace(verb) {
        return None;
    }
    Some(namespace.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest(id: &str) -> ApplicationManifest {
        serde_yaml::from_str(&format!(
            r#"apiVersion: elembra.io/v1alpha1
kind: Application
metadata: {{ id: {id}, name: Test, version: 1.0.0 }}
runtime: {{ kind: embedded }}
contracts: {{ provides: [{{ id: {id}.api, version: v1alpha1 }}] }}
resources: [{{ type: note, actions: [notes.read] }}]
contributions: {{ navigation: [{{ id: notes, label: Home, route: /apps/test }}] }}
configuration: {{ schema: config.json }}
data: {{ owner: {id}, preserveOnDisable: true, exportSupported: true }}
"#
        ))
        .unwrap()
    }
    #[test]
    fn parses_and_validates_manifest() {
        assert!(validate_manifest(&manifest("io.elembra.notes")).is_ok());
    }

    #[test]
    fn future_bridge_application_exposes_canonical_shell_identity() {
        let mut chat = manifest("io.elembra.chat");
        chat.runtime.kind = ApplicationRuntimeKind::Bridge;
        chat.metadata.name = "Chat".to_string();
        chat.resources[0].actions[0] = ActionCapability::from("chat.read");
        chat.contributions.navigation[0] = ApplicationContribution {
            id: "chat.navigation".to_string(),
            label: Some("Chat".to_string()),
            route: Some("/apps/chat".to_string()),
            ..Default::default()
        };

        let registry = ApplicationRegistry::new([chat]).unwrap();
        let chat = registry
            .manifest(&ApplicationId::from("io.elembra.chat"))
            .unwrap();
        assert_eq!(chat.runtime.kind, ApplicationRuntimeKind::Bridge);
        assert_eq!(chat.metadata.id.0, "io.elembra.chat");
        assert_eq!(
            chat.contributions.navigation[0].route.as_deref(),
            Some("/apps/chat")
        );
    }

    #[test]
    fn rejects_contribution_outside_application_namespace() {
        let mut application = manifest("io.elembra.notes");
        application.contributions.navigation[0].id = "chat.navigation".to_string();
        assert!(matches!(
            validate_manifest(&application),
            Err(ApplicationRegistryError::InvalidManifest(_))
        ));
    }
    #[test]
    fn rejects_duplicate_application() {
        let id = manifest("io.elembra.notes");
        assert!(matches!(
            ApplicationRegistry::new([id.clone(), id]),
            Err(ApplicationRegistryError::DuplicateApplication(_))
        ));
    }

    #[test]
    fn rejects_unsatisfied_contract() {
        let mut application = manifest("io.elembra.notes");
        application.contracts.requires.push(ContractRef {
            id: "io.elembra.files.api".into(),
            version: "v1alpha1".into(),
        });
        assert!(matches!(
            ApplicationRegistry::new([application]),
            Err(ApplicationRegistryError::MissingContract(_, _))
        ));
    }

    #[test]
    fn rejects_contribution_collision() {
        let mut application = manifest("io.elembra.notes");
        application
            .contributions
            .routes
            .push(application.contributions.navigation[0].clone());
        assert!(matches!(
            ApplicationRegistry::new([application]),
            Err(ApplicationRegistryError::DuplicateContribution(_))
        ));
    }

    #[test]
    fn allows_multiple_actions_owned_by_one_application() {
        let mut application = manifest("io.elembra.notes");
        application.resources[0]
            .actions
            .push(ActionCapability::from("notes.write"));
        assert!(ApplicationRegistry::new([application]).is_ok());
    }
    #[test]
    fn rejects_invalid_integration_event_declarations() {
        let mut application = manifest("io.elembra.notes");
        application.integration_events.publishes = vec!["garbage".into()];
        assert!(matches!(
            validate_manifest(&application),
            Err(ApplicationRegistryError::InvalidEventType(_))
        ));
        let mut application = manifest("io.elembra.notes");
        application.integration_events.subscribes =
            vec!["io.elembra.mail.message.archived.v1".into()];
        assert!(validate_manifest(&application).is_ok());
    }

    #[test]
    fn files_manifest_owns_file_events() {
        let registry = ApplicationRegistry::first_party().unwrap();
        let files = ApplicationId::new("io.elembra.files");
        let notes = ApplicationId::new("io.elembra.notes");
        let created = "io.elembra.files.file.created.v1";
        let updated = "io.elembra.files.file.updated.v1";

        let files_manifest = registry.manifest(&files).unwrap();
        assert!(files_manifest.publishes_event(created));
        assert!(files_manifest.publishes_event(updated));
        assert!(!files_manifest.publishes_event("io.elembra.mail.message.archived.v1"));

        assert!(registry.owns_event_type(&files, created));
        assert!(registry.owns_event_type(&files, updated));
        assert!(!registry.owns_event_type(&files, "io.elembra.files.file.deleted.v1"));
        assert!(!registry.owns_event_type(&notes, created));
        assert!(!registry.owns_event_type(&notes, updated));
    }

    #[test]
    fn valid_event_type_rule_matches_envelope_validator() {
        assert!(valid_event_type("io.elembra.files.file.created.v1"));
        assert!(valid_event_type("io.elembra.files.v1"));
        assert!(!valid_event_type("garbage"));
        assert!(!valid_event_type("io.elembra.files.file.created.v0"));
        assert!(!valid_event_type("io.elembra.files.file.created"));
        // A version-like segment mid-string must not pass envelope syntax.
        assert!(!valid_event_type("io.elembra.files.v1.v2"));
        assert!(!valid_event_type("io.elembra.files.file.v1.created.v1"));
        // A bare `v` segment counts as version-like too.
        assert!(!valid_event_type("io.elembra.v.file.created.v1"));
        assert!(!valid_event_type("io.elembra.files.v.file.created.v1"));
    }

    #[test]
    fn isolates_enablement_and_preserves_identity_across_runtime_change() {
        let mut a = manifest("io.elembra.notes");
        let id = a.metadata.id.clone();
        let mut r = ApplicationRegistry::new([a.clone()]).unwrap();
        let t = TenantId(Uuid::new_v4());
        let w = WorkspaceId(Uuid::new_v4());
        r.configure(
            t,
            w,
            &id,
            false,
            serde_json::json!({"rootPath":"/Workspace/Notes"}),
        );
        assert_eq!(r.state(t, w, &id), Some(ApplicationState::Configured));
        assert!(r.is_configured(t, w, &id));
        assert!(!r.is_enabled(t, w, &id));
        assert!(!r.enablement(t, w, &id).unwrap().enabled);
        let other_tenant = TenantId(Uuid::new_v4());
        let other_workspace = WorkspaceId(Uuid::new_v4());
        assert_eq!(
            r.state(other_tenant, other_workspace, &id),
            Some(ApplicationState::Available)
        );
        assert!(!r.is_enabled(other_tenant, other_workspace, &id));
        r.configure(
            other_tenant,
            other_workspace,
            &id,
            true,
            serde_json::json!({}),
        );
        assert!(r.is_enabled(other_tenant, other_workspace, &id));
        assert!(!r.is_enabled(t, w, &id));
        assert!(r.set_health(t, w, &id, ApplicationHealth::Degraded));
        a.runtime.kind = ApplicationRuntimeKind::Service;
        assert_eq!(a.metadata.id, id);
    }
}
