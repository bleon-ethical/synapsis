use crate::core::lock_utils::*;
use super::{Task, TaskResult, TaskError};
use std::collections::HashMap;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub struct ExternalAgentConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub timeout_secs: u64,
}

impl Default for ExternalAgentConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            timeout_secs: 300,
        }
    }
}

pub trait ExternalAgentConnector: Send + Sync {
    fn agent_id(&self) -> &str;
    fn agent_type(&self) -> &str;
    fn is_running(&self) -> bool;
    fn spawn(&mut self) -> Result<(), TaskError>;
    fn send_task(&mut self, task: Task) -> Result<(), TaskError>;
    fn poll_result(&mut self) -> Option<TaskResult>;
    fn shutdown(&mut self) -> Result<(), TaskError>;
}

struct ConnectorState {
    process: Option<Child>,
    pending_tasks: Arc<Mutex<HashMap<String, Task>>>,
    running: Arc<Mutex<bool>>,
}

macro_rules! impl_connector {
    ($name:ident, $id:expr, $default_cmd:expr, $default_args:expr) => {
        pub struct $name {
            config: ExternalAgentConfig,
            state: ConnectorState,
        }

        impl $name {
            pub fn new(config: ExternalAgentConfig) -> Self {
                Self {
                    config,
                    state: ConnectorState {
                        process: None,
                        pending_tasks: Arc::new(Mutex::new(HashMap::new())),
                        running: Arc::new(Mutex::new(false)),
                    },
                }
            }

            pub fn with_default_config() -> Self {
                Self::new(ExternalAgentConfig {
                    command: $default_cmd.to_string(),
                    args: $default_args.iter().map(|s| s.to_string()).collect(),
                    env: HashMap::new(),
                    timeout_secs: 300,
                })
            }

            fn spawn_inner(&mut self, agent_name: &str) -> Result<(), TaskError> {
                if *self.state.running.lock_safe() {
                    return Err(TaskError::new(0x0200, &format!("Agent already running"), agent_name));
                }

                let mut cmd = Command::new(&self.config.command);
                for arg in &self.config.args { cmd.arg(arg); }
                for (key, val) in &self.config.env { cmd.env(key, val); }

                cmd.stdin(Stdio::piped());
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());

                let child = cmd.spawn().map_err(|e| {
                    TaskError::execution_failed(&format!("Failed to spawn {}: {}", agent_name, e), agent_name)
                })?;

                self.state.process = Some(child);
                *self.state.running.lock_safe() = true;
                Ok(())
            }

            fn send_task_inner(&mut self, task: Task, agent_name: &str) -> Result<(), TaskError> {
                if !*self.state.running.lock_safe() {
                    return Err(TaskError::new(0x0201, "Agent not running", agent_name));
                }

                let task_json = serde_json::to_string(&task).map_err(|e| {
                    TaskError::new(0x0202, &format!("Failed to serialize task: {}", e), agent_name)
                })?;

                self.state.pending_tasks.lock_safe().insert(task.id.clone(), task);

                if let Some(ref mut child) = self.state.process {
                    if let Some(ref mut stdin) = child.stdin {
                        stdin.write_all(task_json.as_bytes()).map_err(|e| {
                            TaskError::execution_failed(&format!("Failed to send task: {}", e), agent_name)
                        })?;
                    }
                }
                Ok(())
            }
        }
    };
}

impl_connector!(OpenCodeConnector, "opencode", "opencode", ["--agent", "--mode", "pipe"]);
impl_connector!(QwenConnector, "qwen", "qwen", ["--agent", "--pipe"]);
impl_connector!(ClaudeConnector, "claude", "claude", ["--agent", "--pipe"]);

macro_rules! impl_connector_trait {
    ($name:ident, $agent_type:expr) => {
        impl ExternalAgentConnector for $name {
            fn agent_id(&self) -> &str { $agent_type }
            fn agent_type(&self) -> &str { $agent_type }

            fn is_running(&self) -> bool {
                *self.state.running.lock_safe()
            }

            fn spawn(&mut self) -> Result<(), TaskError> {
                self.spawn_inner($agent_type)
            }

            fn send_task(&mut self, task: Task) -> Result<(), TaskError> {
                self.send_task_inner(task, $agent_type)
            }

            fn poll_result(&mut self) -> Option<TaskResult> {
                if !*self.state.running.lock_safe() {
                    return None;
                }
                if let Some(ref mut child) = self.state.process {
                    if let Ok(Some(status)) = child.try_wait() {
                        if status.success() {
                            *self.state.running.lock_safe() = false;
                        }
                    }
                }
                None
            }

            fn shutdown(&mut self) -> Result<(), TaskError> {
                if let Some(ref mut child) = self.state.process {
                    child.kill().map_err(|e| {
                        TaskError::execution_failed(&format!("Failed to kill agent: {}", e), $agent_type)
                    })?;
                }
                self.state.process = None;
                *self.state.running.lock_safe() = false;
                Ok(())
            }
        }
    };
}

impl_connector_trait!(OpenCodeConnector, "opencode");
impl_connector_trait!(QwenConnector, "qwen");
impl_connector_trait!(ClaudeConnector, "claude");
