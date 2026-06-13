//! # Smart Context Management System
//!
//! Sistema de gestión de contextos inteligente con:
//! - Aislamiento de contextos
//! - Contexto global heredable  
//! - Conexiones inteligentes entre contextos
//! - Recycling en frío (no eliminación)
//! - Sistema de orquestación multi-agente
//! - Compresión de contexto y budget tracking

pub mod context_compression;
pub mod context_types;
pub mod orchestration;

pub use context_compression::{
    AlertType, CompressedContext, CompressionLevel, ContentTier, ContextAlert, ContextBudget,
    ContextCompressor, ContextFragment, ContextMonitor,
};
pub use context_types::{
    AccessLevel, Context, ContextId, ContextMetrics, ContextRef, ContextRegistry, ContextState,
    ContextType, ContextValue, IsolationConfig, Priority, SearchResult,
};
pub use orchestration::{
    AgentId, AgentState, AgentType, OrchStatus, Orchestrator, Suggestion, Task, TaskId,
    TaskPriority, TaskResult, TaskState, TaskType,
};
