use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use crate::core::uuid::Uuid;
use crate::domain::types::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Assigned,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> Ordering {
        (*self as u8).cmp(&(*other as u8)).reverse()
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub required_skills: Vec<String>,
    pub priority: Priority,
    pub status: TaskStatus,
    pub assigned_to: Option<String>,
    pub created_at: Timestamp,
    pub deadline: Option<Timestamp>,
    pub timeout_secs: Option<u64>,
    pub result: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
}

impl Task {
    pub fn new(description: String, required_skills: Vec<String>, priority: Priority) -> Self {
        Self {
            id: format!("task-{}", Uuid::new_v4().to_hex_string()),
            description,
            required_skills,
            priority,
            status: TaskStatus::Pending,
            assigned_to: None,
            created_at: Timestamp::now(),
            deadline: None,
            timeout_secs: None,
            result: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    pub fn with_deadline(mut self, deadline: Timestamp) -> Self {
        self.deadline = Some(deadline);
        self
    }

    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = Some(timeout_secs);
        self
    }

    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    pub fn is_expired(&self) -> bool {
        if let Some(deadline) = self.deadline {
            return Timestamp::now() > deadline;
        }
        false
    }

    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub agent_type: String,
    pub skills: Vec<String>,
    pub capacity: u32,
    pub current_load: u32,
    pub last_heartbeat: Timestamp,
    pub is_available: bool,
}

impl AgentInfo {
    pub fn new(id: String, agent_type: String, skills: Vec<String>, capacity: u32) -> Self {
        Self {
            id,
            agent_type,
            skills,
            capacity,
            current_load: 0,
            last_heartbeat: Timestamp::now(),
            is_available: true,
        }
    }

    pub fn has_capacity(&self) -> bool {
        self.current_load < self.capacity && self.is_available
    }

    pub fn matches_skills(&self, required: &[String]) -> bool {
        required.iter().any(|s| self.skills.contains(s))
    }

    pub fn skill_score(&self, required: &[String]) -> usize {
        required.iter().filter(|s| self.skills.contains(s)).count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskQueueEvent {
    pub event_type: TaskQueueEventType,
    pub task_id: String,
    pub agent_id: Option<String>,
    pub timestamp: Timestamp,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskQueueEventType {
    TaskCreated,
    TaskAssigned,
    TaskStarted,
    TaskCompleted,
    TaskFailed,
    TaskRequeued,
    AgentRegistered,
    AgentHeartbeat,
    AgentTimeout,
    AgentUnavailable,
}

pub(crate) struct PriorityTask {
    pub task: Task,
    pub order: u64,
}

impl PartialEq for PriorityTask {
    fn eq(&self, other: &Self) -> bool {
        self.priority().eq(&other.priority()) && self.order.eq(&other.order)
    }
}

impl Eq for PriorityTask {}

impl PartialOrd for PriorityTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityTask {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority()
            .cmp(&other.priority())
            .then_with(|| self.order.cmp(&other.order))
    }
}

impl PriorityTask {
    pub fn new(task: Task, order: u64) -> Self {
        Self { task, order }
    }

    fn priority(&self) -> Priority {
        if self.task.is_expired() {
            return Priority::Critical;
        }
        self.task.priority
    }
}
