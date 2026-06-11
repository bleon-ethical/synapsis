pub type TaskId = String;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: TaskId,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub assignee: Option<String>,
}

impl Task {
    pub fn new(description: String, priority: TaskPriority, assignee: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description,
            status: TaskStatus::Pending,
            priority,
            assignee,
        }
    }
}
