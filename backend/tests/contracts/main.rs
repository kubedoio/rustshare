#![allow(unused_imports)]

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
//! - E-MAP-01 through E-MAP-06: Editor file API mapping tests (`editor_file_api_mapping_contract`)
//! - LB-02: ApplicationConfig tenant and permission contract tests (`application_permission_contract`)
//! - E-STUB-01 through E-STUB-05: Deferred dedicated editor API stub tests
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

// ApplicationConfig declarations
mod ai_permission_contract;
mod application_permission_contract;
mod chat_integration_contract;
pub mod common;
mod device_pairing_contract;
mod editor_file_api_mapping_contract;
mod public_upload_only_contract;
mod restore_contract;
mod search_authorization_contract;
mod share_link_contract;
mod storage_verification_contract;
mod tenant_isolation_contract;
mod vault_sync_contract;
mod versioning_contract;

// Re-export all tests so they are discovered by the test runner
pub use ai_permission_contract::*;
pub use application_permission_contract::*;
pub use chat_integration_contract::*;
pub use device_pairing_contract::*;
pub use editor_file_api_mapping_contract::*;
pub use public_upload_only_contract::*;
pub use restore_contract::*;
pub use search_authorization_contract::*;
pub use share_link_contract::*;
pub use storage_verification_contract::*;
pub use tenant_isolation_contract::*;
pub use vault_sync_contract::*;
pub use versioning_contract::*;
