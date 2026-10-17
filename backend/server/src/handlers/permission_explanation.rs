use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use rustshare_core::{
    domain::SharePermissions,
    services::{PermissionResult, PermissionSource},
};

/// Effective resource access as resolved by the live permission resolver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct EffectivePermissionExplanation {
    pub source: EffectivePermissionSource,
    pub permission: SharePermissions,
    /// Present only for a direct Admin grant, whose id the caller may inspect
    /// through the existing resource grant-management permission gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EffectivePermissionSource {
    Owner,
    DirectShare,
    GroupShare,
    Inherited,
}

impl EffectivePermissionExplanation {
    pub fn from_resolution(result: PermissionResult) -> Option<Self> {
        let permission = result.permission?;
        let source = match result.source {
            PermissionSource::Owner => EffectivePermissionSource::Owner,
            PermissionSource::DirectShare => EffectivePermissionSource::DirectShare,
            PermissionSource::GroupShare => EffectivePermissionSource::GroupShare,
            PermissionSource::Inherited => EffectivePermissionSource::Inherited,
            PermissionSource::None => return None,
        };

        Some(Self {
            source,
            permission,
            share_id: (permission == SharePermissions::Admin
                && result.source == PermissionSource::DirectShare)
                .then_some(result.share_id)
                .flatten(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_direct_admin_exposes_a_share_id() {
        let share_id = Uuid::new_v4();
        let view = EffectivePermissionExplanation::from_resolution(PermissionResult {
            permission: Some(SharePermissions::View),
            source: PermissionSource::GroupShare,
            share_id: Some(share_id),
        })
        .unwrap();
        assert_eq!(view.source, EffectivePermissionSource::GroupShare);
        assert_eq!(view.permission, SharePermissions::View);
        assert_eq!(view.share_id, None);

        let admin = EffectivePermissionExplanation::from_resolution(PermissionResult {
            permission: Some(SharePermissions::Admin),
            source: PermissionSource::DirectShare,
            share_id: Some(share_id),
        })
        .unwrap();
        assert_eq!(admin.share_id, Some(share_id));

        for source in [PermissionSource::GroupShare, PermissionSource::Inherited] {
            let explanation = EffectivePermissionExplanation::from_resolution(PermissionResult {
                permission: Some(SharePermissions::Admin),
                source,
                share_id: Some(share_id),
            })
            .unwrap();
            assert_eq!(explanation.share_id, None);
        }

        assert!(
            EffectivePermissionExplanation::from_resolution(PermissionResult {
                permission: None,
                source: PermissionSource::None,
                share_id: None,
            })
            .is_none()
        );
    }
}
