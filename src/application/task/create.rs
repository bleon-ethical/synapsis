use crate::domain::models::task::{Task, TaskId, TaskPriority};
use crate::domain::ports::storage_port::TaskStorage;

pub struct CreateTask<'a> {
    storage: &'a dyn TaskStorage,
}

impl<'a> CreateTask<'a> {
    pub fn new(storage: &'a dyn TaskStorage) -> Self {
        Self { storage }
    }

    pub fn execute(&self, description: &str, priority: TaskPriority) -> Result<TaskId, String> {
        let task = Task::new(description.to_string(), priority, None);
        let id = task.id.clone();
        self.storage.save_task(
            &task.id,
            &task.description,
            &format!("{:?}", task.priority).to_lowercase(),
        )?;
        Ok(id)
    }
}
