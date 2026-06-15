#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Agent {
    pub id: String,
    pub agent_type: String,
    pub name: String,
    pub skills: Vec<String>,
    pub status: AgentStatus,
    pub current_task: Option<String>,
    pub workload: u32,
    pub created_at: i64,
    pub last_heartbeat: i64,
    pub parent_agent: Option<String>,
    pub sub_agents: Vec<String>,
    pub is_sub_orchestrator: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentStatus {
    Idle,
    Busy,
    Thinking,
    Waiting,
    Disconnected,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub required_skills: Vec<String>,
    pub priority: u8,
    pub assigned_to: Option<String>,
    pub status: TaskStatus,
    pub created_at: i64,
    pub parent_task: Option<String>,
    pub review_required: bool,
    pub reviewed_by: Option<String>,
    pub review_status: Option<ReviewStatus>,
    pub coordinated: bool,
    pub sync_group: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TaskStatus {
    Pending,
    Assigned,
    InProgress,
    Completed,
    Failed,
    Delegated,
    AwaitingReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReviewStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrchestratorMessage {
    pub id: String,
    pub from: String,
    pub to: Option<String>,
    pub message_type: MessageType,
    pub payload: serde_json::Value,
    pub timestamp: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MessageType {
    TaskRequest,
    TaskResponse,
    Delegation,
    SkillOffer,
    SkillRequest,
    Heartbeat,
    StatusUpdate,
    Coordination,
    ReviewRequest,
    ReviewApprove,
    ReviewReject,
    SyncPoint,
    CrossOrchestrator,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LegacyFile {
    pub path: String,
    pub protected: bool,
    pub locked_by: Option<String>,
    pub reason: String,
    pub timestamp: i64,
}
