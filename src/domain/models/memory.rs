pub use synapsis_core::domain::entities::Observation;
pub use synapsis_core::domain::entities::SearchParams;
pub use synapsis_core::domain::types::{ObservationId, ObservationType, SessionId};

pub type MemoryId = String;

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub session_id: SessionId,
    pub content: String,
}

impl MemoryEntry {
    pub fn new(session_id: SessionId, content: String) -> Self {
        Self { session_id, content }
    }
}
