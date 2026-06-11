use crate::domain::models::agent::AgentId;
use crate::domain::ports::agent_port::AgentPort;

pub struct SendHeartbeat<'a> {
    port: &'a dyn AgentPort,
}

impl<'a> SendHeartbeat<'a> {
    pub fn new(port: &'a dyn AgentPort) -> Self {
        Self { port }
    }

    pub fn execute(&self, agent_id: &AgentId) -> Result<(), String> {
        self.port.heartbeat(agent_id)
    }
}
