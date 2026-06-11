use crate::domain::models::memory::{MemoryEntry, MemoryId, SessionId};

pub trait MemoryPort: Send + Sync {
    fn save(&self, entry: MemoryEntry) -> Result<MemoryId, String>;
    fn recall(&self, session_id: &SessionId, limit: usize) -> Result<Vec<String>, String>;
    fn search(&self, query: &str) -> Result<Vec<String>, String>;
}
