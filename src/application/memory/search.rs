use synapsis_core::domain::entities::SearchParams;
use synapsis_core::domain::ports::StoragePort;

pub struct SearchMemories<'a> {
    storage: &'a dyn StoragePort,
}

impl<'a> SearchMemories<'a> {
    pub fn new(storage: &'a dyn StoragePort) -> Self {
        Self { storage }
    }

    pub fn execute(&self, query: &str) -> Result<Vec<String>, String> {
        let params = SearchParams::new(query.to_string());
        let results = self.storage.search_observations(&params).map_err(|e| e.to_string())?;
        Ok(results.into_iter().map(|r| r.observation.content).collect())
    }
}
