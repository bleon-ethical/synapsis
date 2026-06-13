//! Synapsis - Persistent Memory Engine for AI Agents
//!
//! This is the main application crate, building on top of `synapsis-core`
//! to provide the full Synapsis experience with MCP, HTTP, CLI, and TUI interfaces.
#![allow(dead_code, unused_imports)]
#![recursion_limit = "512"]

// Clean Architecture modules (domain, application, infrastructure)
pub mod domain;
pub mod application;
pub mod infrastructure;

// App-specific core logic (updater, resilience, etc.)
pub mod app_core;

// CLI
pub mod cli;

// Presentation layer (MCP, HTTP, CLI, TUI) - specific to synapsis application
pub mod api;
pub mod presentation;
pub mod tools;

// Plugins - extended capabilities
pub mod plugins;

// Session cleanup module - automatic session lifecycle management
pub mod session_cleanup;
pub use synapsis_core::core::session_cleanup::{SessionCleanupConfig, SessionCleanupJob, CleanupStats};

pub use synapsis_core::infrastructure::database::Database;
pub use synapsis_core::domain::ports::StoragePort;

// Re-export core modules explicitly to resolve path issues
pub mod rate_limiter {
    pub use synapsis_core::core::rate_limiter::*;
}

pub mod audit_log {
    pub use synapsis_core::core::audit_log::*;
}

pub mod zero_trust {
    pub use synapsis_core::core::zero_trust::*;
}

// PQC Digital Signatures with CRYSTALS-Dilithium (requires `pqc` feature)
#[cfg(feature = "pqc")]
pub mod dilithium {
    pub use crate::app_core::dilithium::*;
}

// Re-exports for umbra compatibility
pub use domain::models::memory::{Observation, ObservationType, SessionId, SearchParams, MemoryEntry};
pub use synapsis_core::domain::types::Timestamp;
/// Backward compatibility alias
pub use MemoryEntry as Memory;
pub use domain::ports::memory_port::MemoryPort;
