pub type AgentId = String;

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Active,
    Idle,
    Offline,
    Error,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: AgentId,
    pub name: String,
    pub status: AgentStatus,
    pub last_heartbeat: Option<String>,
    pub metadata: Option<String>,
}

impl Agent {
    pub fn new(id: AgentId, name: String) -> Self {
        Self {
            id,
            name,
            status: AgentStatus::Active,
            last_heartbeat: None,
            metadata: None,
        }
    }
}
