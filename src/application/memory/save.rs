use synapsis_core::domain::entities::Observation;
use synapsis_core::domain::ports::StoragePort;
use synapsis_core::domain::types::{ObservationType, SessionId};

pub struct SaveMemory<'a> {
    storage: &'a dyn StoragePort,
}

impl<'a> SaveMemory<'a> {
    pub fn new(storage: &'a dyn StoragePort) -> Self {
        Self { storage }
    }

    pub fn execute(&self, session_id: SessionId, title: &str, content: &str) -> Result<String, String> {
        let obs = Observation::new(session_id, ObservationType::Manual, title.to_string(), content.to_string());
        self.storage.save_observation(&obs).map(|id| id.0).map_err(|e| e.to_string())
    }
}
