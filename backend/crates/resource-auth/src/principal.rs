//! [`PrincipalContext`] — the canonical cross-Application authority context.
//!
//! See ADR-0032 and `docs/specs/resource-ref-authorization-v1alpha1.md`.
//!
//! The transport/workload identity authenticates *who is calling*. The
//! `PrincipalContext` identifies *whose business authority the operation
//! exercises*. A trusted service must never silently gain the user's
//! authority or bypass it.
//!
//! Group IDs, application grants and delegation bounds carried in a context
//! are informational. The receiving trusted boundary (the owner adapter) must
//! derive or verify the authority it relies on — this crate never treats a
//! client-supplied `group_ids`/`grants` array as a grant of access.

use crate::decision::Decision;
use crate::resource_ref::ResourceRef;
use chrono::{DateTime, Utc};
use rustshare_core::domain::{ActionCapability, CorrelationId, PrincipalId, TenantId, WorkspaceId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Maximum number of actions a single delegation may carry.
pub const MAX_DELEGATED_ACTIONS: usize = 64;

/// The kind of business Principal exercising authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    /// A human user.
    User,
    /// A trusted workload/service account. Never a substitute for a Principal.
    Service,
    /// An Agent acting under an explicit delegation. Never impersonates a user.
    Agent,
}

/// Authentication method/strength context (informational).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuthenticationContext {
    /// e.g. `oidc`, `web_session`, `bearer`, `workload`.
    #[serde(default)]
    pub method: String,
    /// OIDC issuer when applicable.
    #[serde(default)]
    pub issuer: Option<String>,
    /// e.g. `mfa`, `password`.
    #[serde(default)]
    pub strength: Option<String>,
}

/// Identity of the transport/workload that authenticated the call.
///
/// This is *not* the business Principal. It is recorded for audit and for
/// explicit service-to-service delegation checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkloadIdentity {
    /// Application identity of the calling workload, if known.
    #[serde(default)]
    pub application_id: Option<rustshare_core::domain::ApplicationId>,
    /// Workload subject/credential identifier.
    #[serde(default)]
    pub subject: Option<String>,
}

/// An explicit, bounded delegation of action capabilities.
///
/// The delegate (Agent/Service) remains the acting Principal; it does not
/// become the issuer. The delegated action set is an upper bound, never an
/// automatic resource allow — the source Application still evaluates
/// resource-level authorization for the issuer's authority.
///
/// # Trusted-boundary contract
///
/// A `Delegation` must only ever be constructed at the trusted boundary from
/// an authoritative grant store or verified signature — never from
/// client-visible state. The issuer's *current* authority is re-evaluated at
/// the source for every request, so a forged delegation to a powerless
/// issuer grants nothing; but the delegation itself (who issued it) must be
/// established by Platform Core, not supplied by the caller.
///
/// `PrincipalContext`/`Delegation` are serializable (see
/// [`PrincipalContext`]) for transport **between trusted components only**. A
/// serialized or client-supplied context is never trusted authorization proof:
/// only a trusted boundary that authenticated the workload/user and
/// verified/reconstructed the context may call the
/// [`SourceAuthorizer`](crate::SourceAuthorizer). The future HTTP/service
/// transports (#212/#213) MUST verify delegation grant and revocation state
/// before constructing a trusted context. `grant_id` is an **audit
/// identifier**: v1alpha1 does not check it against persistent revocation
/// state (grant persistence is deferred to the Agents Application). Grant
/// issuance and verification storage are deferred to the Agents Application;
/// the first consumer that wires a request path must construct
/// `PrincipalContext` only through that gated boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delegation {
    /// The Principal that issued the delegation (the ultimate initiator).
    pub issuer_principal_id: PrincipalId,
    /// The Principal that exercises the delegation.
    pub delegate_principal_id: PrincipalId,
    /// Allowed action capabilities (upper bound).
    #[serde(default)]
    pub actions: Vec<ActionCapability>,
    /// Scope the delegation is valid for.
    #[serde(default)]
    pub workspace_id: Option<WorkspaceId>,
    /// Optional explicit resource restriction; when present, only refs listed
    /// here may be acted on (matched on application/type/id, version ignored).
    #[serde(default)]
    pub resource_scope: Option<Vec<ResourceRef>>,
    /// Delegation expiry; expired delegations fail closed.
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// Stable grant identifier retained in audit records.
    #[serde(default)]
    pub grant_id: Option<String>,
}

/// The canonical authority context carried by cross-Application calls.
///
/// Constructed at the trusted boundary (e.g. from an authenticated handler
/// request). Never populated from untrusted client input. `Serialize` is for
/// transport between trusted components only — deserializing a context is not
/// verification; a client-supplied context is never trusted authorization
/// proof (see [`Delegation`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalContext {
    /// The business Principal exercising authority.
    pub principal_id: PrincipalId,
    /// Kind of the acting Principal.
    pub principal_kind: PrincipalKind,
    /// Tenant scope.
    pub tenant_id: TenantId,
    /// Workspace scope. Today RustShare maps one workspace per tenant
    /// (`WorkspaceId == TenantId`); the [`SourceAuthorizer`](crate::SourceAuthorizer)
    /// fails closed when a context's workspace does not correspond to its
    /// tenant (enforced before any owner is consulted).
    pub workspace_id: WorkspaceId,
    /// Group membership projection. Informational only; owners derive/verify
    /// membership from authoritative state before relying on it.
    #[serde(default)]
    pub group_ids: Vec<Uuid>,
    /// Application-level grants. Informational only; never a resource allow.
    #[serde(default)]
    pub grants: Vec<ActionCapability>,
    /// Authentication method/strength context.
    #[serde(default)]
    pub authentication: Option<AuthenticationContext>,
    /// Active delegation for Service/Agent principals.
    #[serde(default)]
    pub delegation: Option<Delegation>,
    /// Transport/workload identity that authenticated the call.
    #[serde(default)]
    pub workload_identity: Option<WorkloadIdentity>,
    /// Correlation identifier for audit/causation chains.
    #[serde(default)]
    pub correlation_id: Option<CorrelationId>,
}

/// The resolved authority a source owner should evaluate.
///
/// For a human user this is the user themselves. For an Agent/Service it is
/// the delegation issuer — the Agent/Service never becomes the user, but the
/// resource owner evaluates the underlying delegated authority (revocations
/// apply immediately) bounded by the delegation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectivePrincipal {
    /// The acting Principal (user, service or agent).
    pub acting_principal_id: PrincipalId,
    /// The user whose current authority the owner evaluates.
    pub user_authority_id: PrincipalId,
}

/// Why a delegation could not be honored. All variants fail closed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PrincipalError {
    #[error("acting principal is not allowed to carry a delegation")]
    DelegationNotAllowedForUser,
    #[error("delegation is required for this principal kind")]
    MissingDelegation,
    #[error("delegation was issued for a different principal")]
    DelegationMismatch,
    #[error("delegation expired")]
    ExpiredDelegation,
    #[error("action {0} is not within the delegated action set")]
    ActionNotDelegated(String),
    #[error("workspace {0} is outside the delegation scope")]
    WorkspaceOutOfScope(String),
    #[error("resource is outside the delegation scope")]
    ResourceOutOfScope,
    #[error("a principal cannot delegate to itself")]
    ImpersonationForbidden,
    #[error("delegation carries too many actions (limit {MAX_DELEGATED_ACTIONS})")]
    DelegationTooLarge,
}

impl PrincipalContext {
    /// Convenience constructor for a human-user context (no delegation).
    pub fn user(principal_id: PrincipalId, tenant_id: TenantId, workspace_id: WorkspaceId) -> Self {
        Self {
            principal_id,
            principal_kind: PrincipalKind::User,
            tenant_id,
            workspace_id,
            group_ids: Vec::new(),
            grants: Vec::new(),
            authentication: None,
            delegation: None,
            workload_identity: None,
            correlation_id: None,
        }
    }

    /// Resolve the effective authority for an action, using the current clock.
    pub fn effective_user_authority(
        &self,
        action: &ActionCapability,
        resource: Option<&ResourceRef>,
    ) -> Result<EffectivePrincipal, PrincipalError> {
        self.effective_user_authority_at(action, resource, Utc::now())
    }

    /// Resolve the effective authority at an explicit instant (deterministic tests).
    pub fn effective_user_authority_at(
        &self,
        action: &ActionCapability,
        resource: Option<&ResourceRef>,
        now: DateTime<Utc>,
    ) -> Result<EffectivePrincipal, PrincipalError> {
        match self.principal_kind {
            PrincipalKind::User => {
                if self.delegation.is_some() {
                    return Err(PrincipalError::DelegationNotAllowedForUser);
                }
                Ok(EffectivePrincipal {
                    acting_principal_id: self.principal_id,
                    user_authority_id: self.principal_id,
                })
            }
            PrincipalKind::Service | PrincipalKind::Agent => {
                let delegation = self
                    .delegation
                    .as_ref()
                    .ok_or(PrincipalError::MissingDelegation)?;
                self.validate_delegation(delegation, action, resource, now)?;
                Ok(EffectivePrincipal {
                    acting_principal_id: self.principal_id,
                    user_authority_id: delegation.issuer_principal_id,
                })
            }
        }
    }

    /// Audit-oriented helper: the acting principal and, when a delegation is
    /// present, the initiator/grant that bounds it.
    pub fn audit_chain(&self) -> (PrincipalId, Option<(PrincipalId, Option<&str>)>) {
        match &self.delegation {
            Some(delegation) => (
                self.principal_id,
                Some((
                    delegation.issuer_principal_id,
                    delegation.grant_id.as_deref(),
                )),
            ),
            None => (self.principal_id, None),
        }
    }

    fn validate_delegation(
        &self,
        delegation: &Delegation,
        action: &ActionCapability,
        resource: Option<&ResourceRef>,
        now: DateTime<Utc>,
    ) -> Result<(), PrincipalError> {
        if delegation.delegate_principal_id != self.principal_id {
            return Err(PrincipalError::DelegationMismatch);
        }
        if delegation.issuer_principal_id == delegation.delegate_principal_id {
            return Err(PrincipalError::ImpersonationForbidden);
        }
        if delegation.actions.len() > MAX_DELEGATED_ACTIONS {
            return Err(PrincipalError::DelegationTooLarge);
        }
        if let Some(expires_at) = delegation.expires_at {
            if now >= expires_at {
                return Err(PrincipalError::ExpiredDelegation);
            }
        }
        if !delegation.actions.iter().any(|a| a == action) {
            return Err(PrincipalError::ActionNotDelegated(action.0.clone()));
        }
        if let Some(scope_workspace) = delegation.workspace_id {
            if scope_workspace != self.workspace_id {
                return Err(PrincipalError::WorkspaceOutOfScope(
                    self.workspace_id.to_string(),
                ));
            }
        }
        if let Some(scope) = &delegation.resource_scope {
            let resource = resource.ok_or(PrincipalError::ResourceOutOfScope)?;
            let in_scope = scope.iter().any(|candidate| {
                candidate.application == resource.application
                    && candidate.resource_type == resource.resource_type
                    && candidate.resource_id == resource.resource_id
            });
            if !in_scope {
                return Err(PrincipalError::ResourceOutOfScope);
            }
        }
        Ok(())
    }
}

impl EffectivePrincipal {
    /// Convert a policy denial from this layer into a fail-closed decision.
    pub fn to_decision(error: &PrincipalError) -> Decision {
        match error {
            PrincipalError::DelegationTooLarge => Decision::Invalid,
            _ => Decision::Deny,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::ApplicationId;
    use uuid::Uuid;

    fn ctx(
        principal: PrincipalId,
        kind: PrincipalKind,
        delegation: Option<Delegation>,
    ) -> PrincipalContext {
        PrincipalContext {
            principal_id: principal,
            principal_kind: kind,
            tenant_id: TenantId(Uuid::new_v4()),
            workspace_id: WorkspaceId(Uuid::new_v4()),
            group_ids: vec![],
            grants: vec![],
            authentication: None,
            delegation,
            workload_identity: None,
            correlation_id: None,
        }
    }

    fn delegation_for(
        issuer: PrincipalId,
        delegate: PrincipalId,
        actions: &[&str],
        expires_at: Option<DateTime<Utc>>,
    ) -> Delegation {
        Delegation {
            issuer_principal_id: issuer,
            delegate_principal_id: delegate,
            actions: actions.iter().map(|a| ActionCapability::new(*a)).collect(),
            workspace_id: None,
            resource_scope: None,
            expires_at,
            grant_id: Some("grant-1".into()),
        }
    }

    #[test]
    fn user_principal_is_its_own_authority() {
        let user = PrincipalId(Uuid::new_v4());
        let context = ctx(user, PrincipalKind::User, None);
        let effective = context
            .effective_user_authority(&ActionCapability::new(crate::FILES_READ), None)
            .unwrap();
        assert_eq!(effective.acting_principal_id, user);
        assert_eq!(effective.user_authority_id, user);
    }

    #[test]
    fn user_carries_no_delegation() {
        let user = PrincipalId(Uuid::new_v4());
        let context = ctx(
            user,
            PrincipalKind::User,
            Some(delegation_for(user, user, &[], None)),
        );
        assert_eq!(
            context.effective_user_authority(&ActionCapability::new(crate::FILES_READ), None),
            Err(PrincipalError::DelegationNotAllowedForUser)
        );
    }

    #[test]
    fn service_without_delegation_cannot_bypass_principal() {
        let service = PrincipalId(Uuid::new_v4());
        let context = ctx(service, PrincipalKind::Service, None);
        assert_eq!(
            context.effective_user_authority(&ActionCapability::new(crate::FILES_READ), None),
            Err(PrincipalError::MissingDelegation)
        );
    }

    #[test]
    fn agent_with_valid_delegation_acts_for_issuer() {
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let context = ctx(
            agent,
            PrincipalKind::Agent,
            Some(delegation_for(issuer, agent, &[crate::FILES_READ], None)),
        );
        let effective = context
            .effective_user_authority(&ActionCapability::new(crate::FILES_READ), None)
            .unwrap();
        // The agent remains the acting principal; the issuer's authority is
        // what the source owner evaluates.
        assert_eq!(effective.acting_principal_id, agent);
        assert_eq!(effective.user_authority_id, issuer);
    }

    #[test]
    fn agent_cannot_exceed_delegated_actions() {
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let context = ctx(
            agent,
            PrincipalKind::Agent,
            Some(delegation_for(issuer, agent, &[crate::FILES_READ], None)),
        );
        assert_eq!(
            context.effective_user_authority(&ActionCapability::new(crate::FILES_WRITE), None),
            Err(PrincipalError::ActionNotDelegated(
                crate::FILES_WRITE.into()
            ))
        );
    }

    #[test]
    fn expired_delegation_fails_closed() {
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let past = Utc::now() - chrono::Duration::seconds(1);
        let context = ctx(
            agent,
            PrincipalKind::Agent,
            Some(delegation_for(
                issuer,
                agent,
                &[crate::FILES_READ],
                Some(past),
            )),
        );
        assert_eq!(
            context.effective_user_authority(&ActionCapability::new(crate::FILES_READ), None),
            Err(PrincipalError::ExpiredDelegation)
        );
    }

    #[test]
    fn delegation_must_name_the_acting_principal() {
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let other = PrincipalId(Uuid::new_v4());
        let context = ctx(
            agent,
            PrincipalKind::Agent,
            Some(delegation_for(issuer, other, &[crate::FILES_READ], None)),
        );
        assert_eq!(
            context.effective_user_authority(&ActionCapability::new(crate::FILES_READ), None),
            Err(PrincipalError::DelegationMismatch)
        );
    }

    #[test]
    fn self_delegation_is_impersonation() {
        let agent = PrincipalId(Uuid::new_v4());
        let context = ctx(
            agent,
            PrincipalKind::Agent,
            Some(delegation_for(agent, agent, &[crate::FILES_READ], None)),
        );
        assert_eq!(
            context.effective_user_authority(&ActionCapability::new(crate::FILES_READ), None),
            Err(PrincipalError::ImpersonationForbidden)
        );
    }

    #[test]
    fn resource_scope_limits_delegation() {
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let in_scope = ResourceRef::new(
            ApplicationId::new("io.elembra.files"),
            "file",
            "11111111-1111-1111-1111-111111111111",
        );
        let mut delegation = delegation_for(issuer, agent, &[crate::FILES_READ], None);
        delegation.resource_scope = Some(vec![in_scope.clone()]);
        let context = ctx(agent, PrincipalKind::Agent, Some(delegation));

        let allowed = context
            .effective_user_authority(&ActionCapability::new(crate::FILES_READ), Some(&in_scope))
            .unwrap();
        assert_eq!(allowed.user_authority_id, issuer);

        let out_of_scope = ResourceRef::new(
            ApplicationId::new("io.elembra.files"),
            "file",
            "22222222-2222-2222-2222-222222222222",
        );
        assert_eq!(
            context.effective_user_authority(
                &ActionCapability::new(crate::FILES_READ),
                Some(&out_of_scope)
            ),
            Err(PrincipalError::ResourceOutOfScope)
        );
        // A resource must be present when a scope is declared.
        assert_eq!(
            context.effective_user_authority(&ActionCapability::new(crate::FILES_READ), None),
            Err(PrincipalError::ResourceOutOfScope)
        );
    }

    #[test]
    fn workspace_scope_limits_delegation() {
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let mut delegation = delegation_for(issuer, agent, &[crate::FILES_READ], None);
        delegation.workspace_id = Some(WorkspaceId(Uuid::new_v4()));
        let context = ctx(agent, PrincipalKind::Agent, Some(delegation));
        assert!(matches!(
            context.effective_user_authority(&ActionCapability::new(crate::FILES_READ), None),
            Err(PrincipalError::WorkspaceOutOfScope(_))
        ));
    }

    #[test]
    fn delegation_actions_are_bounded() {
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let mut delegation = delegation_for(issuer, agent, &[], None);
        delegation.actions = (0..=MAX_DELEGATED_ACTIONS)
            .map(|i| ActionCapability::new(format!("files.read{i}")))
            .collect();
        let context = ctx(agent, PrincipalKind::Agent, Some(delegation));
        assert_eq!(
            context.effective_user_authority(&ActionCapability::new(crate::FILES_READ), None),
            Err(PrincipalError::DelegationTooLarge)
        );
    }

    #[test]
    fn audit_chain_reports_actor_and_initiator() {
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let context = ctx(
            agent,
            PrincipalKind::Agent,
            Some(delegation_for(issuer, agent, &[crate::FILES_READ], None)),
        );
        let (actor, initiator) = context.audit_chain();
        assert_eq!(actor, agent);
        let (issuer_id, grant) = initiator.unwrap();
        assert_eq!(issuer_id, issuer);
        assert_eq!(grant, Some("grant-1"));
    }

    /// A `PrincipalContext` (with its delegation) round-trips through
    /// serialization. This documents that contexts are transportable values —
    /// which is exactly why a *deserialized* context is not trusted
    /// authorization proof: trust comes only from the boundary that
    /// constructed it.
    #[test]
    fn serialized_context_round_trips_but_is_not_trust_proof() {
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let context = ctx(
            agent,
            PrincipalKind::Agent,
            Some(delegation_for(issuer, agent, &[crate::FILES_READ], None)),
        );
        let json = serde_json::to_string(&context).expect("context serializes");
        let restored: PrincipalContext = serde_json::from_str(&json).expect("context deserializes");
        assert_eq!(restored, context);
        // The deserialized value still carries the same delegated action set;
        // verifying that the issuer really granted it is the trusted
        // boundary's job, not the serialized payload's.
        assert_eq!(restored.delegation, context.delegation);
    }

    /// `grant_id` is an audit identifier, not a revocation check: v1alpha1
    /// evaluates two delegations that differ only in `grant_id` identically.
    /// This pins that no hidden grant/revocation-state lookup exists yet —
    /// future transports (#212/#213) MUST verify grant/revocation before
    /// constructing a trusted context.
    #[test]
    fn grant_id_is_audit_only_not_revocation_state() {
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let mut with_grant = delegation_for(issuer, agent, &[crate::FILES_READ], None);
        with_grant.grant_id = Some("grant-42".into());
        let mut without_grant = delegation_for(issuer, agent, &[crate::FILES_READ], None);
        without_grant.grant_id = None;

        let action = ActionCapability::new(crate::FILES_READ);
        let with_ctx = ctx(agent, PrincipalKind::Agent, Some(with_grant));
        let without_ctx = ctx(agent, PrincipalKind::Agent, Some(without_grant));
        assert_eq!(
            with_ctx.effective_user_authority(&action, None).unwrap(),
            without_ctx.effective_user_authority(&action, None).unwrap()
        );
        // The audit chain carries the grant id for traceability; it is not a
        // live revocation/issuance check.
        let (_, initiator) = with_ctx.audit_chain();
        assert_eq!(initiator.unwrap().1, Some("grant-42"));
        let (_, initiator) = without_ctx.audit_chain();
        assert_eq!(initiator.unwrap().1, None);
    }
}
