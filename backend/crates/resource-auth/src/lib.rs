//! Elembra cross-Application identity and resource authorization contracts.
//!
//! This crate implements the shared contracts defined by ADR-0032 and
//! `docs/specs/resource-ref-authorization-v1alpha1.md`:
//!
//! * [`PrincipalContext`] — the canonical authority context carried by
//!   cross-Application calls. It identifies whose business authority an
//!   operation exercises, separate from transport/workload authentication.
//! * [`ResourceRef`] — a transport-neutral, opaque reference to a resource
//!   owned by exactly one Elembra Application. A ref never grants access.
//! * [`ResourceOwner`] — the source-authorization contract implemented by the
//!   Application that owns a resource type. The owner is the final authority.
//! * [`SourceAuthorizer`] — the Platform-Core-side facade that validates refs,
//!   enforces delegation bounds and routes to the owning Application adapter.
//!
//! Architectural invariants enforced here:
//!
//! * the owning Application is the final authority for resource-level access;
//! * possession of a [`ResourceRef`] grants nothing by itself;
//! * service/workload identity never silently substitutes for a Principal;
//! * Agents act under an explicit delegation and never impersonate a user;
//! * malformed/unknown/cross-tenant references fail closed;
//! * contexts whose workspace does not correspond to their tenant fail closed
//!   (RustShare maps one workspace per tenant today);
//! * content is fetched only after current source authorization.
//!
//! The concrete Files owner adapter lives in `rustshare-server` (the
//! `authz` module) because it delegates to Files' existing, authoritative
//! permission semantics (`PermissionResolver`) and object storage.

pub mod actions;
pub mod authorizer;
pub mod chat_identity;
pub mod contract;
pub mod decision;
pub mod principal;
pub mod registry;
pub mod resource_ref;

pub use actions::*;
pub use authorizer::SourceAuthorizer;
pub use chat_identity::{
    authorize_admission, build_buzz_admission_event, build_buzz_admission_event_at,
    AdmissionDecision, BindingChallenge, BindingError, BindingStatus, BuzzAdmission,
    BuzzAdmissionCommandError, BuzzAdmissionOperation, ChatIdentityBinding,
    WorkspaceCommunityMapping, BUZZ_RELAY_ADD_MEMBER_KIND, BUZZ_RELAY_REMOVE_MEMBER_KIND,
};
pub use contract::{
    Candidate, FetchedResource, MaterializedCandidate, Purpose, Representation, ResolvedResource,
    ResourceCapability, ResourceOwner, SourceError, MAX_BATCH_SIZE,
};
pub use decision::{BatchDecision, Decision};
pub use principal::{
    AuthenticationContext, Delegation, EffectivePrincipal, PrincipalContext, PrincipalError,
    PrincipalKind, WorkloadIdentity,
};
pub use registry::{RegistrationError, ResourceOwnerRegistry};
pub use resource_ref::{RefParseError, ResourceRef};
