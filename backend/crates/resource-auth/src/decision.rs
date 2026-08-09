//! Authorization decision values for the source-authorization contract.

use crate::resource_ref::ResourceRef;
use serde::{Deserialize, Serialize};

/// The decision of a source Application for one authorization request.
///
/// Allowed values per `docs/specs/resource-ref-authorization-v1alpha1.md`:
/// `allow`, `deny`, `not_found`, `invalid`.
///
/// Externally exposed security-sensitive endpoints may deliberately coalesce
/// `deny` and `not_found` to avoid existence leakage. Internal typed APIs may
/// retain the distinction only where policy permits it and callers do not
/// expose it unsafely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// The Principal may perform the action on the resource.
    Allow,
    /// The Principal may not perform the action.
    Deny,
    /// The resource does not exist or is outside the tenant/workspace scope.
    NotFound,
    /// The reference itself is malformed or names an unknown Application/type.
    Invalid,
}

impl Decision {
    /// Whether the decision grants the action.
    pub fn is_allow(self) -> bool {
        matches!(self, Decision::Allow)
    }
}

/// One item of a batch authorization result, explicitly associated with the
/// resource ref it applies to. Results never depend solely on array ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchDecision {
    pub resource: ResourceRef,
    pub decision: Decision,
}

impl BatchDecision {
    pub fn new(resource: ResourceRef, decision: Decision) -> Self {
        Self { resource, decision }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&Decision::NotFound).unwrap(),
            "\"not_found\""
        );
        assert_eq!(
            serde_json::to_string(&Decision::Invalid).unwrap(),
            "\"invalid\""
        );
        assert!(Decision::Allow.is_allow());
        assert!(!Decision::Deny.is_allow());
    }
}
