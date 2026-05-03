//! Contract Tests for RustShare
//!
//! This module contains the mandatory executable test suites as defined in the contract document.
//! Per the contract: "A feature is not considered implemented until the relevant contract is testable"
//!
//! ## Test Organization
//!
//! - G-01: Tenant isolation tests (`tenant_isolation_contract`)
//! - S-01 through S-08: Share link tests (`share_link_contract`)
//! - S-03: Public upload-only tests (`public_upload_only_contract`)
//! - F-01 through F-04: Versioning tests (`versioning_contract`)
//! - F-04 & G-06: Restore and backup tests (`restore_contract`)
//! - Q-01 & Q-02: Search authorization tests (`search_authorization_contract`)
//! - H-01 through H-06: Chat integration tests (`chat_integration_contract`)
//! - C-01 through C-06: Device pairing tests (`device_pairing_contract`)
//! - A-01 through A-07: AI permission tests (`ai_permission_contract`)
//! - ST-01 through ST-06: Storage verification tests (`storage_verification_contract`)
//!
//! ## Running the Tests
//!
//! ```bash
//! # Run all contract tests (requires database and S3)
//! cargo test --test contracts -- --ignored
//!
//! # Run specific contract test suite
//! cargo test --test contracts tenant_isolation -- --ignored
//! cargo test --test contracts share_link -- --ignored
//! ```

// Module declarations
mod contracts {
    pub mod ai_permission_contract;
    pub mod chat_integration_contract;
    pub mod common;
    pub mod device_pairing_contract;
    pub mod public_upload_only_contract;
    pub mod restore_contract;
    pub mod search_authorization_contract;
    pub mod share_link_contract;
    pub mod storage_verification_contract;
    pub mod tenant_isolation_contract;
    pub mod versioning_contract;
}

// Re-export all tests so they are discovered by the test runner
pub use contracts::ai_permission_contract::*;
pub use contracts::chat_integration_contract::*;
pub use contracts::device_pairing_contract::*;
pub use contracts::public_upload_only_contract::*;
pub use contracts::restore_contract::*;
pub use contracts::search_authorization_contract::*;
pub use contracts::share_link_contract::*;
pub use contracts::storage_verification_contract::*;
pub use contracts::tenant_isolation_contract::*;
pub use contracts::versioning_contract::*;
