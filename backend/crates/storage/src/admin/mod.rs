//! Admin tools for metadata management
//!
//! This module provides verification, repair, and rebuild utilities
//! for the metadata system.

pub mod verification;
pub mod repair;
pub mod rebuild;

pub use verification::*;
pub use repair::*;
pub use rebuild::*;

use serde::{Deserialize, Serialize};

/// Summary of an admin operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationSummary {
    pub operation: String,
    pub items_processed: usize,
    pub items_succeeded: usize,
    pub items_failed: usize,
    pub items_fixed: usize,
    pub errors: Vec<String>,
}

impl OperationSummary {
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            items_processed: 0,
            items_succeeded: 0,
            items_failed: 0,
            items_fixed: 0,
            errors: Vec::new(),
        }
    }
    
    pub fn success_rate(&self) -> f64 {
        if self.items_processed == 0 {
            100.0
        } else {
            (self.items_succeeded as f64 / self.items_processed as f64) * 100.0
        }
    }
    
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
        self.items_failed += 1;
    }
    
    pub fn increment_processed(&mut self) {
        self.items_processed += 1;
    }
    
    pub fn increment_succeeded(&mut self) {
        self.items_succeeded += 1;
    }
    
    pub fn increment_fixed(&mut self) {
        self.items_fixed += 1;
    }
}
