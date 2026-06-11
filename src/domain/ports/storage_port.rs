pub use synapsis_core::domain::ports::StoragePort;

pub trait TaskStorage: Send + Sync {
    fn save_task(&self, id: &str, description: &str, priority: &str) -> Result<(), String>;
    fn update_status(&self, id: &str, status: &str) -> Result<(), String>;
}
