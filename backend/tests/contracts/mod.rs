//! Contract Tests common module for use by other integration tests
//!
//! Only re-exports shared helpers so that other test binaries can
//! reference `contracts::common::*` without pulling in the full
//! contract test suites (which use `crate::common` paths).

pub mod common;
pub mod reference_consumer;
pub mod vault_sync_contract;
