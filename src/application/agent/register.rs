use crate::domain::models::agent::{Agent, AgentId};
use crate::domain::ports::agent_port::AgentPort;

pub struct RegisterAgent<'a> {
    port: &'a dyn AgentPort,
}

impl<'a> RegisterAgent<'a> {
    pub fn new(port: &'a dyn AgentPort) -> Self {
        Self { port }
    }

    pub fn execute(&self, agent: Agent) -> Result<AgentId, String> {
        self.port.register(agent)
    }
}

pub struct UnregisterAgent<'a> {
    port: &'a dyn AgentPort,
}

impl<'a> UnregisterAgent<'a> {
    pub fn new(port: &'a dyn AgentPort) -> Self {
        Self { port }
    }

    pub fn execute(&self, agent_id: &AgentId) -> Result<(), String> {
        self.port.unregister(agent_id)
    }
}
