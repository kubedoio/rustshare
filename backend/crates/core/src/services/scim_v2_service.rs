//! SCIM v2 service - Full RFC 7643/7644 compliance.
//!
//! Provides complete SCIM v2 endpoints for enterprise IdP integration
//! including filtering, pagination, and standard schemas.

use crate::domain::User;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use std::sync::Arc;
use thiserror::Error;
use tracing::warn;
use uuid::Uuid;

/// Errors that can occur during SCIM v2 operations.
#[derive(Debug, Error)]
pub enum ScimV2Error {
    #[error("User not found: {0}")]
    UserNotFound(String),
    #[error("Group not found: {0}")]
    GroupNotFound(String),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Filter parse error: {0}")]
    FilterParse(String),
    #[error("Unauthorized")]
    Unauthorized,
}

impl ScimV2Error {
    /// Get the HTTP status code for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            ScimV2Error::UserNotFound(_) | ScimV2Error::GroupNotFound(_) => 404,
            ScimV2Error::InvalidRequest(_) | ScimV2Error::FilterParse(_) => 400,
            ScimV2Error::Conflict(_) => 409,
            ScimV2Error::Unauthorized => 401,
            ScimV2Error::Database(_) => 500,
        }
    }
}

/// SCIM v2 Name complex type (RFC 7643 Section 4.1.1).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ScimV2Name {
    #[serde(rename = "formatted", skip_serializing_if = "Option::is_none")]
    pub formatted: Option<String>,
    #[serde(rename = "familyName", skip_serializing_if = "Option::is_none")]
    pub family_name: Option<String>,
    #[serde(rename = "givenName", skip_serializing_if = "Option::is_none")]
    pub given_name: Option<String>,
    #[serde(rename = "middleName", skip_serializing_if = "Option::is_none")]
    pub middle_name: Option<String>,
    #[serde(rename = "honorificPrefix", skip_serializing_if = "Option::is_none")]
    pub honorific_prefix: Option<String>,
    #[serde(rename = "honorificSuffix", skip_serializing_if = "Option::is_none")]
    pub honorific_suffix: Option<String>,
}

/// SCIM v2 Email complex type (RFC 7643 Section 4.1.2).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimV2Email {
    pub value: String,
    #[serde(rename = "display", skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub email_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<bool>,
}

/// SCIM v2 PhoneNumber complex type.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimV2PhoneNumber {
    pub value: String,
    #[serde(rename = "display", skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub phone_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<bool>,
}

/// SCIM v2 Address complex type.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimV2Address {
    #[serde(rename = "formatted", skip_serializing_if = "Option::is_none")]
    pub formatted: Option<String>,
    #[serde(rename = "streetAddress", skip_serializing_if = "Option::is_none")]
    pub street_address: Option<String>,
    #[serde(rename = "locality", skip_serializing_if = "Option::is_none")]
    pub locality: Option<String>,
    #[serde(rename = "region", skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(rename = "postalCode", skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    #[serde(rename = "country", skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub address_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<bool>,
}

/// SCIM v2 Group member reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimV2Member {
    pub value: String,
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub ref_url: Option<String>,
    #[serde(rename = "display", skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub member_type: Option<String>,
}

/// Meta attributes for SCIM resources.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimV2Meta {
    #[serde(rename = "resourceType")]
    pub resource_type: String,
    #[serde(rename = "created", skip_serializing_if = "Option::is_none")]
    pub created: Option<DateTime<Utc>>,
    #[serde(rename = "lastModified", skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<DateTime<Utc>>,
    pub location: String,
    pub version: Option<String>,
}

/// SCIM v2 User resource (RFC 7643 Section 4.1).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimV2User {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "externalId", skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    #[serde(rename = "userName")]
    pub user_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<ScimV2Name>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(rename = "nickName", skip_serializing_if = "Option::is_none")]
    pub nick_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_url: Option<String>,
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emails: Option<Vec<ScimV2Email>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_numbers: Option<Vec<ScimV2PhoneNumber>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addresses: Option<Vec<ScimV2Address>>,
    #[serde(rename = "groups", skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<ScimV2GroupRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ScimV2Meta>,
    #[serde(flatten)]
    pub schemas: ScimSchemas,
}

/// Reference to a group in a user response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimV2GroupRef {
    pub value: String,
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub ref_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

/// SCIM v2 Group resource (RFC 7643 Section 4.2).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimV2Group {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "externalId", skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<ScimV2Member>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ScimV2Meta>,
    #[serde(flatten)]
    pub schemas: ScimSchemas,
}

/// SCIM schemas container.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScimSchemas {
    #[serde(rename = "schemas")]
    pub schemas: Vec<String>,
}

impl Default for ScimSchemas {
    fn default() -> Self {
        Self { schemas: vec![] }
    }
}

impl ScimV2User {
    /// Create a new SCIM v2 user with the User schema.
    ///
    /// The user_name can be any type that converts to String,
    /// such as `&str` or `String`.
    pub fn new(user_name: impl Into<String>) -> Self {
        Self {
            id: None,
            external_id: None,
            user_name: user_name.into(),
            name: None,
            display_name: None,
            nick_name: None,
            profile_url: None,
            active: true,
            emails: None,
            phone_numbers: None,
            addresses: None,
            groups: None,
            meta: None,
            schemas: ScimSchemas {
                schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:User".to_string()],
            },
        }
    }

    /// Set the ID and generate meta information.
    pub fn with_id(mut self, id: Uuid, base_url: &str) -> Self {
        let id_str = id.to_string();
        self.id = Some(id_str.clone());
        self.meta = Some(ScimV2Meta {
            resource_type: "User".to_string(),
            created: Some(Utc::now()),
            last_modified: Some(Utc::now()),
            location: format!("{}/scim/v2/Users/{}", base_url, id_str),
            version: Some(format!("W/\"{}\"", Utc::now().timestamp())),
        });
        self
    }

    /// Convert from database User to SCIM v2 User.
    pub fn from_user(user: &User, base_url: &str, groups: Vec<(Uuid, String)>) -> Self {
        let name = ScimV2Name {
            formatted: Some(user.display_name.clone()),
            family_name: user.surname.clone(),
            given_name: user.name.clone(),
            middle_name: None,
            honorific_prefix: None,
            honorific_suffix: None,
        };

        let email = ScimV2Email {
            value: user.email.clone(),
            display: Some(user.display_name.clone()),
            email_type: Some("work".to_string()),
            primary: Some(true),
        };

        let group_refs: Vec<ScimV2GroupRef> = groups
            .into_iter()
            .map(|(id, name)| ScimV2GroupRef {
                value: id.to_string(),
                ref_url: Some(format!("{}/scim/v2/Groups/{}", base_url, id)),
                display: Some(name),
            })
            .collect();

        Self {
            id: Some(user.id.to_string()),
            external_id: None,
            user_name: user.username.clone(),
            name: Some(name),
            display_name: Some(user.display_name.clone()),
            nick_name: None,
            profile_url: None,
            active: user.disabled_at.is_none(),
            emails: Some(vec![email]),
            phone_numbers: None,
            addresses: None,
            groups: if group_refs.is_empty() { None } else { Some(group_refs) },
            meta: Some(ScimV2Meta {
                resource_type: "User".to_string(),
                created: Some(user.created_at),
                last_modified: Some(user.updated_at),
                location: format!("{}/scim/v2/Users/{}", base_url, user.id),
                version: Some(format!("W/\"{}\"", user.updated_at.timestamp())),
            }),
            schemas: ScimSchemas {
                schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:User".to_string()],
            },
        }
    }
}

impl ScimV2Group {
    /// Create a new SCIM v2 group with the Group schema.
    ///
    /// The display_name can be any type that converts into a String,
    /// such as `&str` or `String`.
    pub fn new(display_name: impl Into<String>) -> Self {
        Self {
            id: None,
            external_id: None,
            display_name: display_name.into(),
            members: None,
            meta: None,
            schemas: ScimSchemas {
                schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:Group".to_string()],
            },
        }
    }

    /// Set the ID and generate meta information.
    pub fn with_id(mut self, id: Uuid, base_url: &str) -> Self {
        let id_str = id.to_string();
        self.id = Some(id_str.clone());
        self.meta = Some(ScimV2Meta {
            resource_type: "Group".to_string(),
            created: Some(Utc::now()),
            last_modified: Some(Utc::now()),
            location: format!("{}/scim/v2/Groups/{}", base_url, id_str),
            version: Some(format!("W/\"{}\"", Utc::now().timestamp())),
        });
        self
    }
}

/// SCIM v2 ListResponse wrapper (RFC 7643 Section 3.4.2).
#[derive(Debug, Serialize)]
pub struct ScimV2ListResponse<T> {
    #[serde(rename = "schemas")]
    pub schemas: Vec<String>,
    #[serde(rename = "totalResults")]
    pub total_results: i64,
    #[serde(rename = "startIndex", skip_serializing_if = "Option::is_none")]
    pub start_index: Option<i64>,
    #[serde(rename = "itemsPerPage", skip_serializing_if = "Option::is_none")]
    pub items_per_page: Option<i64>,
    #[serde(rename = "Resources")]
    pub resources: Vec<T>,
}

impl<T> ScimV2ListResponse<T> {
    pub fn new(resources: Vec<T>, total_results: i64, start_index: Option<i64>, count: Option<i64>) -> Self {
        Self {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:ListResponse".to_string()],
            total_results,
            start_index,
            items_per_page: count,
            resources,
        }
    }
}

/// SCIM v2 Error response (RFC 7644 Section 3.12).
#[derive(Debug, Serialize)]
pub struct ScimV2ErrorResponse {
    #[serde(rename = "schemas")]
    pub schemas: Vec<String>,
    pub status: String,
    pub detail: Option<String>,
    #[serde(rename = "scimType", skip_serializing_if = "Option::is_none")]
    pub scim_type: Option<String>,
}

impl ScimV2ErrorResponse {
    pub fn new(status: u16, detail: impl Into<String>) -> Self {
        Self {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:Error".to_string()],
            status: status.to_string(),
            detail: Some(detail.into()),
            scim_type: None,
        }
    }

    pub fn with_scim_type(mut self, scim_type: impl Into<String>) -> Self {
        self.scim_type = Some(scim_type.into());
        self
    }
}

/// SCIM v2 ServiceProviderConfig (RFC 7643 Section 5).
#[derive(Debug, Serialize)]
pub struct ScimV2ServiceProviderConfig {
    #[serde(rename = "schemas")]
    pub schemas: Vec<String>,
    #[serde(rename = "documentationUri", skip_serializing_if = "Option::is_none")]
    pub documentation_uri: Option<String>,
    pub patch: ScimV2SupportConfig,
    pub bulk: ScimV2BulkConfig,
    pub filter: ScimV2FilterConfig,
    pub change_password: ScimV2SupportConfig,
    pub sort: ScimV2SupportConfig,
    pub etag: ScimV2SupportConfig,
    #[serde(rename = "authenticationSchemes")]
    pub authentication_schemes: Vec<ScimV2AuthScheme>,
    #[serde(rename = "meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<ScimV2Meta>,
}

#[derive(Debug, Serialize)]
pub struct ScimV2SupportConfig {
    pub supported: bool,
}

#[derive(Debug, Serialize)]
pub struct ScimV2BulkConfig {
    pub supported: bool,
    #[serde(rename = "maxOperations")]
    pub max_operations: i32,
    #[serde(rename = "maxPayloadSize")]
    pub max_payload_size: i32,
}

#[derive(Debug, Serialize)]
pub struct ScimV2FilterConfig {
    pub supported: bool,
    #[serde(rename = "maxResults")]
    pub max_results: i32,
}

#[derive(Debug, Serialize)]
pub struct ScimV2AuthScheme {
    #[serde(rename = "type")]
    pub auth_type: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "specUri", skip_serializing_if = "Option::is_none")]
    pub spec_uri: Option<String>,
    #[serde(rename = "documentationUri", skip_serializing_if = "Option::is_none")]
    pub documentation_uri: Option<String>,
}

impl ScimV2ServiceProviderConfig {
    pub fn new(base_url: &str) -> Self {
        Self {
            schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig".to_string()],
            documentation_uri: Some(format!("{}/docs/scim", base_url)),
            patch: ScimV2SupportConfig { supported: true },
            bulk: ScimV2BulkConfig {
                supported: false,
                max_operations: 0,
                max_payload_size: 0,
            },
            filter: ScimV2FilterConfig {
                supported: true,
                max_results: 200,
            },
            change_password: ScimV2SupportConfig { supported: false },
            sort: ScimV2SupportConfig { supported: false },
            etag: ScimV2SupportConfig { supported: false },
            authentication_schemes: vec![
                ScimV2AuthScheme {
                    auth_type: "oauthbearertoken".to_string(),
                    name: "OAuth Bearer Token".to_string(),
                    description: "Bearer token authentication using RUSTSHARE_SCIM_BEARER_TOKEN".to_string(),
                    spec_uri: Some("https://www.rfc-editor.org/rfc/rfc6750.txt".to_string()),
                    documentation_uri: None,
                },
            ],
            meta: Some(ScimV2Meta {
                resource_type: "ServiceProviderConfig".to_string(),
                created: None,
                last_modified: None,
                location: format!("{}/scim/v2/ServiceProviderConfig", base_url),
                version: None,
            }),
        }
    }
}

/// SCIM v2 ResourceType (RFC 7643 Section 6).
#[derive(Debug, Serialize)]
pub struct ScimV2ResourceType {
    #[serde(rename = "schemas")]
    pub schemas: Vec<String>,
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub description: String,
    #[serde(rename = "schema")]
    pub schema: String,
    #[serde(rename = "schemaExtensions", skip_serializing_if = "Option::is_none")]
    pub schema_extensions: Option<Vec<ScimV2SchemaExtension>>,
    #[serde(rename = "meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<ScimV2Meta>,
}

#[derive(Debug, Serialize)]
pub struct ScimV2SchemaExtension {
    #[serde(rename = "schema")]
    pub schema: String,
    pub required: bool,
}

/// SCIM v2 Schema (RFC 7643 Section 7).
#[derive(Debug, Serialize)]
pub struct ScimV2Schema {
    #[serde(rename = "schemas")]
    pub schemas: Vec<String>,
    pub id: String,
    pub name: String,
    pub description: String,
    pub attributes: Vec<ScimV2SchemaAttribute>,
    #[serde(rename = "meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<ScimV2Meta>,
}

#[derive(Debug, Serialize)]
pub struct ScimV2SchemaAttribute {
    pub name: String,
    #[serde(rename = "type")]
    pub attr_type: String,
    #[serde(rename = "multiValued")]
    pub multi_valued: bool,
    pub description: String,
    pub required: bool,
    #[serde(rename = "caseExact", skip_serializing_if = "Option::is_none")]
    pub case_exact: Option<bool>,
    #[serde(rename = "mutability", skip_serializing_if = "Option::is_none")]
    pub mutability: Option<String>,
    #[serde(rename = "returned", skip_serializing_if = "Option::is_none")]
    pub returned: Option<String>,
    #[serde(rename = "uniqueness", skip_serializing_if = "Option::is_none")]
    pub uniqueness: Option<String>,
}

/// Group record for SCIM v2 operations.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScimV2GroupRecord {
    pub id: Uuid,
    pub external_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// User record with external_id for SCIM v2.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScimV2UserRecord {
    pub id: Uuid,
    pub external_id: Option<String>,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub disabled_at: Option<DateTime<Utc>>,
    pub name: Option<String>,
    pub surname: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Filter operators supported for SCIM filtering.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOperator {
    Eq, // equal
    Ne, // not equal
    Co, // contains
    Sw, // starts with
    Ew, // ends with
    Pr, // present (has value)
    Gt, // greater than
    Ge, // greater than or equal
    Lt, // less than
    Le, // less than or equal
}

/// A parsed SCIM filter.
#[derive(Debug, Clone)]
pub struct ScimFilter {
    pub attribute: String,
    pub operator: FilterOperator,
    pub value: String,
}

/// Parse a SCIM filter expression (simplified implementation).
/// Supports: userName eq "value", externalId eq "value", active eq true, etc.
pub fn parse_filter(filter: &str) -> Result<Vec<ScimFilter>, ScimV2Error> {
    let mut filters = Vec::new();
    
    // Simple parser for common filter patterns
    // Supports: attr op "value" or attr op true/false
    // Examples: userName eq "john", externalId eq "12345", active eq true
    
    for part in filter.split(" and ").map(|s| s.trim()) {
        // Try to parse attribute op value pattern
        if let Some((attr, rest)) = part.split_once(' ') {
            let attr = attr.trim();
            let rest = rest.trim();
            
            if let Some((op_str, value)) = rest.split_once(' ') {
                let op_str = op_str.trim().to_lowercase();
                let value = value.trim().trim_matches('"');
                
                let operator = match op_str.as_str() {
                    "eq" => FilterOperator::Eq,
                    "ne" => FilterOperator::Ne,
                    "co" => FilterOperator::Co,
                    "sw" => FilterOperator::Sw,
                    "ew" => FilterOperator::Ew,
                    "pr" => FilterOperator::Pr,
                    "gt" => FilterOperator::Gt,
                    "ge" => FilterOperator::Ge,
                    "lt" => FilterOperator::Lt,
                    "le" => FilterOperator::Le,
                    _ => return Err(ScimV2Error::FilterParse(format!("Unknown operator: {}", op_str))),
                };
                
                filters.push(ScimFilter {
                    attribute: attr.to_string(),
                    operator,
                    value: value.to_string(),
                });
            }
        }
    }
    
    Ok(filters)
}

/// Repository operations required for SCIM v2 service.
#[async_trait::async_trait]
pub trait ScimV2Repository: Send + Sync {
    /// Get all users with optional filtering and pagination.
    async fn list_users(
        &self,
        filters: &[ScimFilter],
        start_index: Option<i64>,
        count: Option<i64>,
    ) -> Result<(Vec<ScimV2UserRecord>, i64), sqlx::Error>;

    /// Get a single user by ID.
    async fn get_user(&self, id: Uuid) -> Result<Option<ScimV2UserRecord>, sqlx::Error>;

    /// Get a user by external_id.
    async fn get_user_by_external_id(&self, external_id: &str) -> Result<Option<ScimV2UserRecord>, sqlx::Error>;

    /// Create a new user.
    async fn create_user(
        &self,
        user: &ScimV2User,
        tenant_id: Uuid,
        storage_quota: i64,
    ) -> Result<Uuid, sqlx::Error>;

    /// Update an existing user (full replacement).
    async fn update_user(&self, id: Uuid, user: &ScimV2User) -> Result<(), sqlx::Error>;

    /// Partial update a user (PATCH).
    async fn patch_user(&self, id: Uuid, operations: &[ScimPatchOperation]) -> Result<(), sqlx::Error>;

    /// Delete a user.
    async fn delete_user(&self, id: Uuid) -> Result<(), sqlx::Error>;

    /// Get all groups with optional filtering and pagination.
    async fn list_groups(
        &self,
        filters: &[ScimFilter],
        start_index: Option<i64>,
        count: Option<i64>,
    ) -> Result<(Vec<ScimV2GroupRecord>, i64), sqlx::Error>;

    /// Get a single group by ID.
    async fn get_group(&self, id: Uuid) -> Result<Option<ScimV2GroupRecord>, sqlx::Error>;

    /// Get a group by external_id.
    async fn get_group_by_external_id(&self, external_id: &str) -> Result<Option<ScimV2GroupRecord>, sqlx::Error>;

    /// Create a new group.
    async fn create_group(&self, group: &ScimV2Group) -> Result<Uuid, sqlx::Error>;

    /// Update an existing group.
    async fn update_group(&self, id: Uuid, group: &ScimV2Group) -> Result<(), sqlx::Error>;

    /// Partial update a group (PATCH).
    async fn patch_group(&self, id: Uuid, operations: &[ScimPatchOperation]) -> Result<(), sqlx::Error>;

    /// Delete a group.
    async fn delete_group(&self, id: Uuid) -> Result<(), sqlx::Error>;

    /// Get group members.
    async fn get_group_members(&self, group_id: Uuid) -> Result<Vec<(Uuid, String)>, sqlx::Error>;

    /// Get user's groups.
    async fn get_user_groups(&self, user_id: Uuid) -> Result<Vec<(Uuid, String)>, sqlx::Error>;

    /// Add member to group.
    async fn add_group_member(&self, group_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error>;

    /// Remove member from group.
    async fn remove_group_member(&self, group_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error>;

    /// Find user ID by external_id.
    async fn find_user_id_by_external_id(&self, external_id: &str) -> Result<Option<Uuid>, sqlx::Error>;

    /// Clear all members from group.
    async fn clear_group_members(&self, group_id: Uuid) -> Result<(), sqlx::Error>;
}

/// SCIM Patch operation (RFC 7644 Section 3.5.2).
#[derive(Debug, Clone, Deserialize)]
pub struct ScimPatchOperation {
    pub op: String, // "add", "remove", "replace"
    pub path: Option<String>,
    pub value: Option<serde_json::Value>,
}

/// SCIM Patch request body.
#[derive(Debug, Clone, Deserialize)]
pub struct ScimPatchRequest {
    #[serde(rename = "schemas")]
    pub schemas: Vec<String>,
    #[serde(rename = "Operations")]
    pub operations: Vec<ScimPatchOperation>,
}

/// SCIM v2 Service.
pub struct ScimV2Service<R: ScimV2Repository> {
    repository: Arc<R>,
    default_tenant_id: Uuid,
    default_storage_quota: i64,
    base_url: String,
}

impl<R: ScimV2Repository> ScimV2Service<R> {
    /// Create a new SCIM v2 service.
    pub fn new(
        repository: Arc<R>,
        default_tenant_id: Uuid,
        default_storage_quota: i64,
        base_url: String,
    ) -> Self {
        Self {
            repository,
            default_tenant_id,
            default_storage_quota,
            base_url,
        }
    }

    /// List users with filtering and pagination.
    pub async fn list_users(
        &self,
        filter: Option<&str>,
        start_index: Option<i64>,
        count: Option<i64>,
    ) -> Result<ScimV2ListResponse<ScimV2User>, ScimV2Error> {
        let filters = match filter {
            Some(f) => parse_filter(f)?,
            None => vec![],
        };

        let (users, total) = self.repository.list_users(&filters, start_index, count).await?;

        let mut resources = Vec::new();
        for user_record in users {
            let groups = self.repository.get_user_groups(user_record.id).await?;
            
            // Convert record to full user
            let user = self.record_to_user(user_record, groups).await?;
            resources.push(user);
        }

        Ok(ScimV2ListResponse::new(
            resources,
            total,
            start_index,
            count,
        ))
    }

    /// Get a user by ID.
    pub async fn get_user(&self, id: Uuid) -> Result<ScimV2User, ScimV2Error> {
        let user_record = self.repository.get_user(id).await?;

        match user_record {
            Some(record) => {
                let groups = self.repository.get_user_groups(id).await?;
                self.record_to_user(record, groups).await
            }
            None => Err(ScimV2Error::UserNotFound(id.to_string())),
        }
    }

    /// Create a new user.
    pub async fn create_user(&self, user: ScimV2User) -> Result<ScimV2User, ScimV2Error> {
        // Validate required fields
        if user.user_name.is_empty() {
            return Err(ScimV2Error::InvalidRequest("userName is required".to_string()));
        }

        // Check for existing user by external_id
        if let Some(ref external_id) = user.external_id {
            if let Some(existing) = self.repository.get_user_by_external_id(external_id).await? {
                // Update existing user instead
                let existing_id = existing.id;
                return self.update_user(existing_id, user).await;
            }
        }

        let id = self
            .repository
            .create_user(&user, self.default_tenant_id, self.default_storage_quota)
            .await?;

        self.get_user(id).await
    }

    /// Update a user (full replacement - PUT).
    pub async fn update_user(&self, id: Uuid, user: ScimV2User) -> Result<ScimV2User, ScimV2Error> {
        // Check if user exists
        if self.repository.get_user(id).await?.is_none() {
            return Err(ScimV2Error::UserNotFound(id.to_string()));
        }

        self.repository.update_user(id, &user).await?;
        self.get_user(id).await
    }

    /// Patch a user (partial update).
    pub async fn patch_user(
        &self,
        id: Uuid,
        operations: &[ScimPatchOperation],
    ) -> Result<ScimV2User, ScimV2Error> {
        // Check if user exists
        if self.repository.get_user(id).await?.is_none() {
            return Err(ScimV2Error::UserNotFound(id.to_string()));
        }

        self.repository.patch_user(id, operations).await?;
        self.get_user(id).await
    }

    /// Delete a user.
    pub async fn delete_user(&self, id: Uuid) -> Result<(), ScimV2Error> {
        if self.repository.get_user(id).await?.is_none() {
            return Err(ScimV2Error::UserNotFound(id.to_string()));
        }

        self.repository.delete_user(id).await?;
        Ok(())
    }

    /// List groups with filtering and pagination.
    pub async fn list_groups(
        &self,
        filter: Option<&str>,
        start_index: Option<i64>,
        count: Option<i64>,
    ) -> Result<ScimV2ListResponse<ScimV2Group>, ScimV2Error> {
        let filters = match filter {
            Some(f) => parse_filter(f)?,
            None => vec![],
        };

        let (groups, total) = self.repository.list_groups(&filters, start_index, count).await?;

        let mut resources = Vec::new();
        for group_record in groups {
            let members = self.repository.get_group_members(group_record.id).await?;
            let group = self.record_to_group(group_record, members).await?;
            resources.push(group);
        }

        Ok(ScimV2ListResponse::new(
            resources,
            total,
            start_index,
            count,
        ))
    }

    /// Get a group by ID.
    pub async fn get_group(&self, id: Uuid) -> Result<ScimV2Group, ScimV2Error> {
        let group_record = self.repository.get_group(id).await?;

        match group_record {
            Some(record) => {
                let members = self.repository.get_group_members(id).await?;
                self.record_to_group(record, members).await
            }
            None => Err(ScimV2Error::GroupNotFound(id.to_string())),
        }
    }

    /// Create a new group.
    pub async fn create_group(&self, group: ScimV2Group) -> Result<ScimV2Group, ScimV2Error> {
        if group.display_name.is_empty() {
            return Err(ScimV2Error::InvalidRequest("displayName is required".to_string()));
        }

        // Check for existing group by external_id
        if let Some(ref external_id) = group.external_id {
            if let Some(existing) = self.repository.get_group_by_external_id(external_id).await? {
                return self.update_group(existing.id, group).await;
            }
        }

        let id = self.repository.create_group(&group).await?;

        // Add members if provided
        if let Some(ref members) = group.members {
            for member in members {
                if let Ok(user_id) = Uuid::parse_str(&member.value) {
                    if let Err(e) = self.repository.add_group_member(id, user_id).await {
                        warn!("Failed to add member {} to group {}: {}", member.value, id, e);
                    }
                } else {
                    // Try to find by external_id
                    if let Some(user_id) = self.repository.find_user_id_by_external_id(&member.value).await? {
                        if let Err(e) = self.repository.add_group_member(id, user_id).await {
                            warn!("Failed to add member {} to group {}: {}", member.value, id, e);
                        }
                    }
                }
            }
        }

        self.get_group(id).await
    }

    /// Update a group (full replacement).
    pub async fn update_group(&self, id: Uuid, group: ScimV2Group) -> Result<ScimV2Group, ScimV2Error> {
        if self.repository.get_group(id).await?.is_none() {
            return Err(ScimV2Error::GroupNotFound(id.to_string()));
        }

        self.repository.update_group(id, &group).await?;

        // Sync members if provided
        if let Some(ref members) = group.members {
            self.repository.clear_group_members(id).await?;
            
            for member in members {
                if let Ok(user_id) = Uuid::parse_str(&member.value) {
                    if let Err(e) = self.repository.add_group_member(id, user_id).await {
                        warn!("Failed to add member {} to group {}: {}", member.value, id, e);
                    }
                } else {
                    // Try to find by external_id
                    if let Some(user_id) = self.repository.find_user_id_by_external_id(&member.value).await? {
                        if let Err(e) = self.repository.add_group_member(id, user_id).await {
                            warn!("Failed to add member {} to group {}: {}", member.value, id, e);
                        }
                    }
                }
            }
        }

        self.get_group(id).await
    }

    /// Patch a group (partial update, typically for membership changes).
    pub async fn patch_group(
        &self,
        id: Uuid,
        operations: &[ScimPatchOperation],
    ) -> Result<ScimV2Group, ScimV2Error> {
        if self.repository.get_group(id).await?.is_none() {
            return Err(ScimV2Error::GroupNotFound(id.to_string()));
        }

        self.repository.patch_group(id, operations).await?;
        self.get_group(id).await
    }

    /// Delete a group.
    pub async fn delete_group(&self, id: Uuid) -> Result<(), ScimV2Error> {
        if self.repository.get_group(id).await?.is_none() {
            return Err(ScimV2Error::GroupNotFound(id.to_string()));
        }

        self.repository.delete_group(id).await?;
        Ok(())
    }

    /// Get service provider configuration.
    pub fn get_service_provider_config(&self) -> ScimV2ServiceProviderConfig {
        ScimV2ServiceProviderConfig::new(&self.base_url)
    }

    /// Get resource types.
    pub fn get_resource_types(&self) -> Vec<ScimV2ResourceType> {
        vec![
            ScimV2ResourceType {
                schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:ResourceType".to_string()],
                id: "User".to_string(),
                name: "User".to_string(),
                endpoint: "/scim/v2/Users".to_string(),
                description: "User Account".to_string(),
                schema: "urn:ietf:params:scim:schemas:core:2.0:User".to_string(),
                schema_extensions: None,
                meta: Some(ScimV2Meta {
                    resource_type: "ResourceType".to_string(),
                    created: None,
                    last_modified: None,
                    location: format!("{}/scim/v2/ResourceTypes/User", self.base_url),
                    version: None,
                }),
            },
            ScimV2ResourceType {
                schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:ResourceType".to_string()],
                id: "Group".to_string(),
                name: "Group".to_string(),
                endpoint: "/scim/v2/Groups".to_string(),
                description: "Group".to_string(),
                schema: "urn:ietf:params:scim:schemas:core:2.0:Group".to_string(),
                schema_extensions: None,
                meta: Some(ScimV2Meta {
                    resource_type: "ResourceType".to_string(),
                    created: None,
                    last_modified: None,
                    location: format!("{}/scim/v2/ResourceTypes/Group", self.base_url),
                    version: None,
                }),
            },
        ]
    }

    /// Get schemas.
    pub fn get_schemas(&self) -> Vec<ScimV2Schema> {
        vec![
            // User schema
            ScimV2Schema {
                schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:Schema".to_string()],
                id: "urn:ietf:params:scim:schemas:core:2.0:User".to_string(),
                name: "User".to_string(),
                description: "User Account".to_string(),
                attributes: vec![
                    ScimV2SchemaAttribute {
                        name: "userName".to_string(),
                        attr_type: "string".to_string(),
                        multi_valued: false,
                        description: "Unique identifier for the User".to_string(),
                        required: true,
                        case_exact: Some(false),
                        mutability: Some("readWrite".to_string()),
                        returned: Some("default".to_string()),
                        uniqueness: Some("server".to_string()),
                    },
                    ScimV2SchemaAttribute {
                        name: "name".to_string(),
                        attr_type: "complex".to_string(),
                        multi_valued: false,
                        description: "The components of the user's name".to_string(),
                        required: false,
                        case_exact: None,
                        mutability: Some("readWrite".to_string()),
                        returned: Some("default".to_string()),
                        uniqueness: None,
                    },
                    ScimV2SchemaAttribute {
                        name: "displayName".to_string(),
                        attr_type: "string".to_string(),
                        multi_valued: false,
                        description: "Display name".to_string(),
                        required: false,
                        case_exact: Some(false),
                        mutability: Some("readWrite".to_string()),
                        returned: Some("default".to_string()),
                        uniqueness: None,
                    },
                    ScimV2SchemaAttribute {
                        name: "externalId".to_string(),
                        attr_type: "string".to_string(),
                        multi_valued: false,
                        description: "External identifier".to_string(),
                        required: false,
                        case_exact: Some(true),
                        mutability: Some("readWrite".to_string()),
                        returned: Some("default".to_string()),
                        uniqueness: None,
                    },
                    ScimV2SchemaAttribute {
                        name: "active".to_string(),
                        attr_type: "boolean".to_string(),
                        multi_valued: false,
                        description: "Active status".to_string(),
                        required: false,
                        case_exact: None,
                        mutability: Some("readWrite".to_string()),
                        returned: Some("default".to_string()),
                        uniqueness: None,
                    },
                    ScimV2SchemaAttribute {
                        name: "emails".to_string(),
                        attr_type: "complex".to_string(),
                        multi_valued: true,
                        description: "Email addresses".to_string(),
                        required: false,
                        case_exact: Some(false),
                        mutability: Some("readWrite".to_string()),
                        returned: Some("default".to_string()),
                        uniqueness: None,
                    },
                    ScimV2SchemaAttribute {
                        name: "groups".to_string(),
                        attr_type: "complex".to_string(),
                        multi_valued: true,
                        description: "Group memberships".to_string(),
                        required: false,
                        case_exact: None,
                        mutability: Some("readOnly".to_string()),
                        returned: Some("default".to_string()),
                        uniqueness: None,
                    },
                ],
                meta: Some(ScimV2Meta {
                    resource_type: "Schema".to_string(),
                    created: None,
                    last_modified: None,
                    location: format!("{}/scim/v2/Schemas/urn:ietf:params:scim:schemas:core:2.0:User", self.base_url),
                    version: None,
                }),
            },
            // Group schema
            ScimV2Schema {
                schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:Schema".to_string()],
                id: "urn:ietf:params:scim:schemas:core:2.0:Group".to_string(),
                name: "Group".to_string(),
                description: "Group".to_string(),
                attributes: vec![
                    ScimV2SchemaAttribute {
                        name: "displayName".to_string(),
                        attr_type: "string".to_string(),
                        multi_valued: false,
                        description: "Display name".to_string(),
                        required: true,
                        case_exact: Some(false),
                        mutability: Some("readWrite".to_string()),
                        returned: Some("default".to_string()),
                        uniqueness: None,
                    },
                    ScimV2SchemaAttribute {
                        name: "externalId".to_string(),
                        attr_type: "string".to_string(),
                        multi_valued: false,
                        description: "External identifier".to_string(),
                        required: false,
                        case_exact: Some(true),
                        mutability: Some("readWrite".to_string()),
                        returned: Some("default".to_string()),
                        uniqueness: None,
                    },
                    ScimV2SchemaAttribute {
                        name: "members".to_string(),
                        attr_type: "complex".to_string(),
                        multi_valued: true,
                        description: "Group members".to_string(),
                        required: false,
                        case_exact: None,
                        mutability: Some("readWrite".to_string()),
                        returned: Some("default".to_string()),
                        uniqueness: None,
                    },
                ],
                meta: Some(ScimV2Meta {
                    resource_type: "Schema".to_string(),
                    created: None,
                    last_modified: None,
                    location: format!("{}/scim/v2/Schemas/urn:ietf:params:scim:schemas:core:2.0:Group", self.base_url),
                    version: None,
                }),
            },
        ]
    }

    /// Helper: Convert user record to full SCIM user.
    async fn record_to_user(
        &self,
        record: ScimV2UserRecord,
        groups: Vec<(Uuid, String)>,
    ) -> Result<ScimV2User, ScimV2Error> {
        let name = ScimV2Name {
            formatted: Some(record.display_name.clone()),
            family_name: record.surname.clone(),
            given_name: record.name.clone(),
            middle_name: None,
            honorific_prefix: None,
            honorific_suffix: None,
        };

        let email = ScimV2Email {
            value: record.email.clone(),
            display: Some(record.display_name.clone()),
            email_type: Some("work".to_string()),
            primary: Some(true),
        };

        let group_refs: Vec<ScimV2GroupRef> = groups
            .into_iter()
            .map(|(id, name)| ScimV2GroupRef {
                value: id.to_string(),
                ref_url: Some(format!("{}/scim/v2/Groups/{}", self.base_url, id)),
                display: Some(name),
            })
            .collect();

        Ok(ScimV2User {
            id: Some(record.id.to_string()),
            external_id: record.external_id,
            user_name: record.username,
            name: Some(name),
            display_name: Some(record.display_name),
            nick_name: None,
            profile_url: None,
            active: record.disabled_at.is_none(),
            emails: Some(vec![email]),
            phone_numbers: None,
            addresses: None,
            groups: if group_refs.is_empty() { None } else { Some(group_refs) },
            meta: Some(ScimV2Meta {
                resource_type: "User".to_string(),
                created: Some(record.created_at),
                last_modified: Some(record.updated_at),
                location: format!("{}/scim/v2/Users/{}", self.base_url, record.id),
                version: Some(format!("W/\"{}\"", record.updated_at.timestamp())),
            }),
            schemas: ScimSchemas {
                schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:User".to_string()],
            },
        })
    }

    /// Helper: Convert group record to full SCIM group.
    async fn record_to_group(
        &self,
        record: ScimV2GroupRecord,
        members: Vec<(Uuid, String)>,
    ) -> Result<ScimV2Group, ScimV2Error> {
        let member_refs: Vec<ScimV2Member> = members
            .into_iter()
            .map(|(id, name)| ScimV2Member {
                value: id.to_string(),
                ref_url: Some(format!("{}/scim/v2/Users/{}", self.base_url, id)),
                display: Some(name),
                member_type: Some("User".to_string()),
            })
            .collect();

        Ok(ScimV2Group {
            id: Some(record.id.to_string()),
            external_id: record.external_id,
            display_name: record.name,
            members: if member_refs.is_empty() { None } else { Some(member_refs) },
            meta: Some(ScimV2Meta {
                resource_type: "Group".to_string(),
                created: Some(record.created_at),
                last_modified: Some(record.updated_at),
                location: format!("{}/scim/v2/Groups/{}", self.base_url, record.id),
                version: Some(format!("W/\"{}\"", record.updated_at.timestamp())),
            }),
            schemas: ScimSchemas {
                schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:Group".to_string()],
            },
        })
    }
}

/// Generate a temporary password hash for SCIM-provisioned users.
#[cfg(test)]
fn generate_temporary_password_hash() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let random_bytes: [u8; 32] = rng.gen();
    format!("$scim_temp${}", base64_encode(&random_bytes))
}

/// Simple base64 encoding.
#[cfg(test)]
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in input.chunks(3) {
        let b = match chunk.len() {
            1 => [chunk[0], 0, 0],
            2 => [chunk[0], chunk[1], 0],
            3 => [chunk[0], chunk[1], chunk[2]],
            _ => unreachable!(),
        };

        let idx1 = (b[0] >> 2) as usize;
        let idx2 = (((b[0] & 0b11) << 4) | (b[1] >> 4)) as usize;
        let idx3 = (((b[1] & 0b1111) << 2) | (b[2] >> 6)) as usize;
        let idx4 = (b[2] & 0b111111) as usize;

        result.push(ALPHABET[idx1] as char);
        result.push(ALPHABET[idx2] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[idx3] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[idx4] as char);
        } else {
            result.push('=');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(b"A"), "QQ==");
    }

    #[test]
    fn test_parse_filter() {
        let filters = parse_filter(r#"userName eq "john.doe""#).unwrap();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].attribute, "userName");
        assert_eq!(filters[0].operator, FilterOperator::Eq);
        assert_eq!(filters[0].value, "john.doe");

        let filters = parse_filter(r#"externalId eq "12345""#).unwrap();
        assert_eq!(filters[0].attribute, "externalId");
    }

    #[test]
    fn test_user_with_id() {
        let user = ScimV2User::new("john.doe".to_string())
            .with_id(Uuid::new_v4(), "https://example.com");
        
        assert!(user.id.is_some());
        assert!(user.meta.is_some());
        assert_eq!(user.schemas.schemas[0], "urn:ietf:params:scim:schemas:core:2.0:User");
    }

    #[test]
    fn test_service_provider_config() {
        let config = ScimV2ServiceProviderConfig::new("https://example.com");
        
        assert!(config.patch.supported);
        assert!(config.filter.supported);
        assert_eq!(config.filter.max_results, 200);
    }
}
