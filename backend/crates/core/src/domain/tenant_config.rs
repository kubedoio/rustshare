//! Tenant-level sharing configuration

use serde::{Deserialize, Serialize};

/// Who can see share recipients
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum RecipientVisibility {
    /// Only admins see full recipient list (privacy-preserving, default)
    AdminOnly,
    /// Everyone sees all recipients (transparent)
    AllRecipients,
    /// Users see self + same-group members
    SameGroupOnly,
}

impl Default for RecipientVisibility {
    fn default() -> Self {
        RecipientVisibility::AdminOnly
    }
}

impl std::str::FromStr for RecipientVisibility {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "AdminOnly" => Ok(RecipientVisibility::AdminOnly),
            "AllRecipients" => Ok(RecipientVisibility::AllRecipients),
            "SameGroupOnly" => Ok(RecipientVisibility::SameGroupOnly),
            _ => Err(format!("Invalid recipient visibility: {}", s)),
        }
    }
}

impl std::fmt::Display for RecipientVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecipientVisibility::AdminOnly => write!(f, "AdminOnly"),
            RecipientVisibility::AllRecipients => write!(f, "AllRecipients"),
            RecipientVisibility::SameGroupOnly => write!(f, "SameGroupOnly"),
        }
    }
}
