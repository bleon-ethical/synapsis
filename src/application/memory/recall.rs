use synapsis_core::domain::ports::StoragePort;

pub struct RecallMemories<'a> {
    storage: &'a dyn StoragePort,
}

impl<'a> RecallMemories<'a> {
    pub fn new(storage: &'a dyn StoragePort) -> Self {
        Self { storage }
    }

    pub fn execute(&self, limit: usize) -> Result<Vec<String>, String> {
        let results = self.storage.recent_observations(limit).map_err(|e| e.to_string())?;
        Ok(results.into_iter().map(|o| o.content).collect())
    }
}
