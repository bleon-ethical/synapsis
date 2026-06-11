use crate::domain::models::task::TaskId;
use crate::domain::ports::storage_port::TaskStorage;

pub struct CompleteTask<'a> {
    storage: &'a dyn TaskStorage,
}

impl<'a> CompleteTask<'a> {
    pub fn new(storage: &'a dyn TaskStorage) -> Self {
        Self { storage }
    }

    pub fn execute(&self, task_id: &TaskId) -> Result<(), String> {
        self.storage.update_status(task_id, "completed")
    }
}

pub struct CancelTask<'a> {
    storage: &'a dyn TaskStorage,
}

impl<'a> CancelTask<'a> {
    pub fn new(storage: &'a dyn TaskStorage) -> Self {
        Self { storage }
    }

    pub fn execute(&self, task_id: &TaskId) -> Result<(), String> {
        self.storage.update_status(task_id, "cancelled")
    }
}
