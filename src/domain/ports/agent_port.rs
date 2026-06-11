use crate::domain::models::agent::{Agent, AgentId, AgentStatus};

pub trait AgentPort: Send + Sync {
    fn register(&self, agent: Agent) -> Result<AgentId, String>;
    fn unregister(&self, agent_id: &AgentId) -> Result<(), String>;
    fn get(&self, agent_id: &AgentId) -> Result<Option<Agent>, String>;
    fn heartbeat(&self, agent_id: &AgentId) -> Result<(), String>;
}
