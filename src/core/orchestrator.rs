use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::core::uuid::Uuid;
use crate::core::resource_manager::{ResourceManager, AgentLimits};

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

pub struct Orchestrator {
    agents: Arc<Mutex<HashMap<String, Agent>>>,
    tasks: Arc<Mutex<HashMap<String, Task>>>,
    messages: Arc<Mutex<Vec<OrchestratorMessage>>>,
    skills_index: Arc<Mutex<HashMap<String, Vec<String>>>>,
    resource_manager: Arc<ResourceManager>,
    legacy_files: Arc<Mutex<HashMap<String, LegacyFile>>>,
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl Orchestrator {
    pub fn new() -> Self {
        let resource_manager = Arc::new(ResourceManager::new());

        resource_manager.set_agent_limits("opencode", AgentLimits {
            max_concurrent_tasks: 3, max_cpu_percent: 50.0, max_memory_mb: 2048, priority: 8,
        });
        resource_manager.set_agent_limits("qwen", AgentLimits {
            max_concurrent_tasks: 2, max_cpu_percent: 70.0, max_memory_mb: 4096, priority: 7,
        });
        resource_manager.set_agent_limits("qwen-code", AgentLimits {
            max_concurrent_tasks: 2, max_cpu_percent: 60.0, max_memory_mb: 3072, priority: 9,
        });

        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            messages: Arc::new(Mutex::new(Vec::new())),
            skills_index: Arc::new(Mutex::new(HashMap::new())),
            resource_manager,
            legacy_files: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn new_with_persistence(data_dir: &Path) -> Self {
        let orch = Self::new();
        orch.load(data_dir);
        let limits_path = data_dir.join("resource_limits.json");
        if limits_path.exists() {
            let _ = orch.resource_manager.load_limits(&limits_path);
        }
        orch
    }

    pub fn save(&self, data_dir: &Path) {
        let agents = self.agents.lock().unwrap();
        let tasks = self.tasks.lock().unwrap();
        if let Ok(data) = serde_json::to_string_pretty(&*agents) {
            let _ = std::fs::write(data_dir.join("orch_agents.json"), data);
        }
        if let Ok(data) = serde_json::to_string_pretty(&*tasks) {
            let _ = std::fs::write(data_dir.join("orch_tasks.json"), data);
        }
    }

    fn load(&self, data_dir: &Path) {
        if let Ok(data) = std::fs::read_to_string(data_dir.join("orch_agents.json")) {
            if let Ok(agents) = serde_json::from_str::<HashMap<String, Agent>>(&data) {
                let mut a = self.agents.lock().unwrap();
                let mut index = self.skills_index.lock().unwrap();
                for (id, agent) in agents.iter() {
                    for skill in &agent.skills {
                        index.entry(skill.clone()).or_default().push(id.clone());
                    }
                }
                *a = agents;
            }
        }
        if let Ok(data) = std::fs::read_to_string(data_dir.join("orch_tasks.json")) {
            if let Ok(tasks) = serde_json::from_str::<HashMap<String, Task>>(&data) {
                *self.tasks.lock().unwrap() = tasks;
            }
        }
    }

    // ── Agent Hierarchy ────────────────────────────────────────────

    pub fn register_agent(&self, agent_type: &str, skills: Vec<String>) -> String {
        let id = format!("{}-{}", agent_type, Uuid::new_v4().to_hex_string());
        let now = timestamp_now();
        let agent = Agent {
            id: id.clone(), agent_type: agent_type.to_string(),
            name: format!("{}_{}", agent_type, &id[..8]), skills: skills.clone(),
            status: AgentStatus::Idle, current_task: None, workload: 0,
            created_at: now, last_heartbeat: now,
            parent_agent: None, sub_agents: Vec::new(), is_sub_orchestrator: false,
        };
        self.agents.lock().unwrap().insert(id.clone(), agent);
        self.resource_manager.register_agent(&id, None);
        for skill in &skills {
            self.skills_index.lock().unwrap().entry(skill.clone()).or_default().push(id.clone());
        }
        self.log_message(&id, None, MessageType::Coordination, serde_json::json!({"action": "registered", "skills": skills}));
        id
    }

    pub fn register_agent_with_id(&self, agent_id: &str, agent_type: &str, skills: Vec<String>) {
        let now = timestamp_now();
        let agent = Agent {
            id: agent_id.to_string(), agent_type: agent_type.to_string(),
            name: format!("{}_{}", agent_type, &agent_id[..8]), skills: skills.clone(),
            status: AgentStatus::Idle, current_task: None, workload: 0,
            created_at: now, last_heartbeat: now,
            parent_agent: None, sub_agents: Vec::new(), is_sub_orchestrator: false,
        };
        self.agents.lock().unwrap().insert(agent_id.to_string(), agent);
        for skill in &skills {
            self.skills_index.lock().unwrap().entry(skill.clone()).or_default().push(agent_id.to_string());
        }
        self.log_message(agent_id, None, MessageType::Coordination, serde_json::json!({"action": "registered", "skills": skills}));
    }

    /// Register as sub-orchestrator (can manage sub-agents)
    pub fn register_sub_orchestrator(&self, agent_id: &str, agent_type: &str, skills: Vec<String>) {
        let now = timestamp_now();
        let mut agents = self.agents.lock().unwrap();
        if agents.contains_key(agent_id) {
            if let Some(a) = agents.get_mut(agent_id) {
                a.is_sub_orchestrator = true;
            }
        } else {
            let agent = Agent {
                id: agent_id.to_string(), agent_type: agent_type.to_string(),
                name: format!("{}_suborch_{}", agent_type, &agent_id[..8]), skills: skills.clone(),
                status: AgentStatus::Idle, current_task: None, workload: 0,
                created_at: now, last_heartbeat: now,
                parent_agent: None, sub_agents: Vec::new(), is_sub_orchestrator: true,
            };
            agents.insert(agent_id.to_string(), agent);
        }
        drop(agents);
        for skill in &skills {
            self.skills_index.lock().unwrap().entry(skill.clone()).or_default().push(agent_id.to_string());
        }
    }

    /// Attach a sub-agent to a parent agent (parent must be sub-orchestrator)
    pub fn attach_sub_agent(&self, parent_id: &str, sub_id: &str) -> bool {
        let mut agents = self.agents.lock().unwrap();
        let is_orch = agents.get(parent_id).map(|a| a.is_sub_orchestrator).unwrap_or(false);
        if !is_orch { return false; }
        if !agents.contains_key(sub_id) { return false; }
        if let Some(parent) = agents.get_mut(parent_id) {
            if parent.sub_agents.contains(&sub_id.to_string()) { return true; }
            parent.sub_agents.push(sub_id.to_string());
        }
        if let Some(sub) = agents.get_mut(sub_id) {
            sub.parent_agent = Some(parent_id.to_string());
        }
        true
    }

    /// Get all sub-agents of a parent (recursive)
    pub fn get_sub_agent_tree(&self, agent_id: &str) -> Vec<Agent> {
        let agents = self.agents.lock().unwrap();
        let mut result = Vec::new();
        if let Some(parent) = agents.get(agent_id) {
            for sub_id in &parent.sub_agents {
                if let Some(sub) = agents.get(sub_id) {
                    result.push(sub.clone());
                    if sub.is_sub_orchestrator {
                        let deeper = self.get_sub_agent_tree_inner(&agents, sub_id);
                        result.extend(deeper);
                    }
                }
            }
        }
        result
    }

    fn get_sub_agent_tree_inner<'a>(&self, agents: &std::sync::MutexGuard<'a, HashMap<String, Agent>>, agent_id: &str) -> Vec<Agent> {
        let mut result = Vec::new();
        if let Some(parent) = agents.get(agent_id) {
            for sub_id in &parent.sub_agents {
                if let Some(sub) = agents.get(sub_id) {
                    result.push(sub.clone());
                    if sub.is_sub_orchestrator {
                        let deeper = self.get_sub_agent_tree_inner(agents, sub_id);
                        result.extend(deeper);
                    }
                }
            }
        }
        result
    }

    /// Communicate with any agent in the hierarchy (cross-orchestrator)
    pub fn send_cross_message(&self, from: &str, to: &str, content: serde_json::Value) -> String {
        self.send_message(from, Some(to), MessageType::CrossOrchestrator, content)
    }

    /// Get all agents in the hierarchy that can handle a task
    pub fn find_agent_in_hierarchy(&self, skills_needed: &[String], from_agent: &str) -> Option<String> {
        let agents = self.agents.lock().unwrap();
        let mut candidates: Vec<&Agent> = agents.values()
            .filter(|a| (a.status == AgentStatus::Idle || a.status == AgentStatus::Thinking) && a.id != from_agent)
            .filter(|a| skills_needed.iter().any(|s| a.skills.contains(s)))
            .collect();
        candidates.sort_by_key(|a| a.workload);
        candidates.first().map(|a| a.id.clone())
    }

    // ── Review Workflow ────────────────────────────────────────────

    /// Create a task that requires review after completion
    pub fn create_reviewable_task(&self, description: &str, required_skills: Vec<String>, priority: u8, parent: Option<&str>) -> String {
        let id = format!("task-{}", Uuid::new_v4().to_hex_string());
        let now = timestamp_now();
        let task = Task {
            id: id.clone(), description: description.to_string(), required_skills, priority,
            assigned_to: None, status: TaskStatus::Pending, created_at: now, parent_task: parent.map(String::from),
            review_required: true, reviewed_by: None, review_status: None,
            coordinated: false, sync_group: None,
        };
        self.tasks.lock().unwrap().insert(id.clone(), task);
        id
    }

    /// Complete a task and mark it for review
    pub fn complete_task_for_review(&self, task_id: &str, agent_id: &str) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get_mut(task_id) {
            if !task.review_required {
                task.status = TaskStatus::Completed;
                return true;
            }
            task.status = TaskStatus::AwaitingReview;
            task.review_status = Some(ReviewStatus::Pending);
            drop(tasks);
            self.log_message(agent_id, None, MessageType::ReviewRequest, serde_json::json!({
                "task_id": task_id, "action": "review_required", "by": agent_id,
            }));
            true
        } else {
            false
        }
    }

    /// Approve a completed task
    pub fn approve_task(&self, task_id: &str, reviewer_id: &str) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get_mut(task_id) {
            if task.review_status != Some(ReviewStatus::Pending) { return false; }
            task.status = TaskStatus::Completed;
            task.review_status = Some(ReviewStatus::Approved);
            task.reviewed_by = Some(reviewer_id.to_string());
            drop(tasks);
            self.log_message(reviewer_id, None, MessageType::ReviewApprove, serde_json::json!({
                "task_id": task_id, "action": "approved",
            }));
            true
        } else { false }
    }

    /// Reject a completed task (reopens it)
    pub fn reject_task(&self, task_id: &str, reviewer_id: &str, reason: &str) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get_mut(task_id) {
            if task.review_status != Some(ReviewStatus::Pending) { return false; }
            task.status = TaskStatus::Assigned;
            task.review_status = Some(ReviewStatus::Rejected);
            task.reviewed_by = Some(reviewer_id.to_string());
            drop(tasks);
            self.log_message(reviewer_id, None, MessageType::ReviewReject, serde_json::json!({
                "task_id": task_id, "action": "rejected", "reason": reason,
            }));
            true
        } else { false }
    }

    // ── Coordinated Tasks (multi-agent sync) ───────────────────────

    /// Create a coordinated task that requires multiple agents
    pub fn create_coordinated_task(&self, description: &str, sync_group: &str, priority: u8) -> String {
        let id = format!("coord-{}", Uuid::new_v4().to_hex_string());
        let now = timestamp_now();
        let task = Task {
            id: id.clone(), description: description.to_string(), required_skills: vec!["coordinated".into()],
            priority, assigned_to: None, status: TaskStatus::Pending, created_at: now,
            parent_task: None, review_required: false, reviewed_by: None, review_status: None,
            coordinated: true, sync_group: Some(sync_group.to_string()),
        };
        self.tasks.lock().unwrap().insert(id.clone(), task);
        id
    }

    /// Agents announce they reached sync point
    pub fn agent_sync(&self, agent_id: &str, sync_group: &str) -> Vec<String> {
        self.log_message(agent_id, None, MessageType::SyncPoint, serde_json::json!({
            "sync_group": sync_group, "action": "synced",
        }));
        let agents = self.agents.lock().unwrap();
        let in_group: Vec<String> = agents.values()
            .filter(|a| a.current_task.as_deref().map(|t| t.contains(sync_group)).unwrap_or(false))
            .map(|a| a.id.clone())
            .collect();
        in_group
    }

    // ── Legacy Code Protection ──────────────────────────────────────

    /// Protect a legacy file from uncoordinated modification
    pub fn protect_legacy_file(&self, path: &str, reason: &str) {
        let mut files = self.legacy_files.lock().unwrap();
        files.insert(path.to_string(), LegacyFile {
            path: path.to_string(), protected: true, locked_by: None,
            reason: reason.to_string(), timestamp: timestamp_now(),
        });
    }

    /// Lock a legacy file for modification (agent must request)
    pub fn lock_legacy_file(&self, path: &str, agent_id: &str) -> bool {
        let mut files = self.legacy_files.lock().unwrap();
        if let Some(file) = files.get_mut(path) {
            if file.locked_by.is_some() { return false; }
            file.locked_by = Some(agent_id.to_string());
            true
        } else {
            false
        }
    }

    /// Unlock a legacy file
    pub fn unlock_legacy_file(&self, path: &str, agent_id: &str) -> bool {
        let mut files = self.legacy_files.lock().unwrap();
        if let Some(file) = files.get_mut(path) {
            if file.locked_by.as_deref() == Some(agent_id) {
                file.locked_by = None;
                return true;
            }
        }
        false
    }

    /// Check if a legacy file can be modified
    pub fn can_modify_legacy(&self, path: &str, agent_id: &str) -> Result<(), String> {
        let files = self.legacy_files.lock().unwrap();
        if let Some(file) = files.get(path) {
            if !file.protected { return Ok(()); }
            if let Some(ref locker) = file.locked_by {
                if locker != agent_id {
                    return Err(format!("File '{}' locked by {}", path, locker));
                }
            } else {
                return Err(format!("File '{}' is legacy-protected. Request lock first.", path));
            }
        }
        Ok(())
    }

    /// Get all legacy files and their status
    pub fn get_legacy_files(&self) -> Vec<LegacyFile> {
        self.legacy_files.lock().unwrap().values().cloned().collect()
    }

    // ── Inter-Agent Communication ───────────────────────────────────

    /// Send a direct message between any two agents (cross-orchestrator safe)
    pub fn send_agent_message(&self, from: &str, to: &str, content: &str) -> String {
        self.send_message(from, Some(to), MessageType::Coordination, serde_json::json!({
            "content": content, "type": "direct_message",
        }))
    }

    /// Broadcast message to all agents in a subtree (sub-orchestrator -> its sub-agents)
    pub fn broadcast_to_subtree(&self, from_orchestrator: &str, content: &str) -> Vec<String> {
        let subs = self.get_sub_agent_tree(from_orchestrator);
        let ids: Vec<String> = subs.iter().map(|a| a.id.clone()).collect();
        for sub_id in &ids {
            self.send_message(from_orchestrator, Some(sub_id), MessageType::Coordination, serde_json::json!({
                "content": content, "type": "broadcast",
            }));
        }
        ids
    }

    /// Check if two agents are in the same hierarchy (share a root orchestrator)
    pub fn are_in_same_hierarchy(&self, agent_a: &str, agent_b: &str) -> bool {
        let agents = self.agents.lock().unwrap();
        let get_root = |id: &str| -> Option<String> {
            let mut current = id.to_string();
            loop {
                let a = agents.get(&current)?;
                match &a.parent_agent {
                    Some(p) => current = p.clone(),
                    None => return Some(current),
                }
            }
        };
        get_root(agent_a) == get_root(agent_b)
    }

    // ── Existing methods (kept with updates) ────────────────────────

    pub fn unregister_agent(&self, agent_id: &str) {
        let mut agents = self.agents.lock().unwrap();
        if let Some(agent) = agents.remove(agent_id) {
            let mut index = self.skills_index.lock().unwrap();
            for skill in &agent.skills {
                if let Some(agent_list) = index.get_mut(skill) {
                    agent_list.retain(|a| a != agent_id);
                }
            }
        }
    }

    pub fn heartbeat(&self, agent_id: &str, status: Option<AgentStatus>, task: Option<&str>) {
        let was_idle = {
            let agents = self.agents.lock().unwrap();
            agents.get(agent_id).map(|a| a.status == AgentStatus::Idle).unwrap_or(false)
        };
        let mut agents = self.agents.lock().unwrap();
        if let Some(agent) = agents.get_mut(agent_id) {
            agent.last_heartbeat = timestamp_now();
            if let Some(s) = status { agent.status = s; }
            if let Some(t) = task { agent.current_task = Some(t.to_string()); }
        }
        drop(agents);
        if was_idle || status == Some(AgentStatus::Idle) {
            self.proactive_assign_to(agent_id);
        }
    }

    pub fn proactive_assign_to(&self, agent_id: &str) -> Option<Task> {
        let (agent_status, agent_skills) = {
            let agents = self.agents.lock().unwrap();
            match agents.get(agent_id) {
                Some(a) => (a.status, a.skills.clone()),
                None => return None,
            }
        };
        if agent_status != AgentStatus::Idle { return None; }
        self.get_pending_tasks().into_iter().find(|task| {
            task.required_skills.iter().any(|s| agent_skills.contains(s))
                && self.assign_task(&task.id, agent_id)
        })
    }

    pub fn proactive_assign_all(&self) -> Vec<(String, Task)> {
        let mut assigned = Vec::new();
        for agent in self.get_idle_agents() {
            if let Some(task) = self.proactive_assign_to(&agent.id) {
                assigned.push((agent.id.clone(), task));
            }
        }
        assigned
    }

    pub fn get_agent_task_notification(&self, agent_id: &str) -> Option<serde_json::Value> {
        let messages = self.get_agent_messages(agent_id, 0);
        messages.into_iter()
            .find(|m| matches!(m.message_type, MessageType::TaskResponse))
            .map(|m| m.payload)
    }

    pub fn create_task(&self, description: &str, required_skills: Vec<String>, priority: u8, parent: Option<&str>) -> String {
        let id = format!("task-{}", Uuid::new_v4().to_hex_string());
        let now = timestamp_now();
        let task = Task {
            id: id.clone(), description: description.to_string(), required_skills, priority,
            assigned_to: None, status: TaskStatus::Pending, created_at: now, parent_task: parent.map(String::from),
            review_required: false, reviewed_by: None, review_status: None,
            coordinated: false, sync_group: None,
        };
        self.tasks.lock().unwrap().insert(id.clone(), task);
        id
    }

    pub fn find_best_agent(&self, skills_needed: &[String]) -> Option<String> {
        let agents = self.agents.lock().unwrap();
        let mut candidates: Vec<&Agent> = agents.values()
            .filter(|a| a.status == AgentStatus::Idle || a.status == AgentStatus::Thinking)
            .filter(|a| skills_needed.iter().any(|s| a.skills.contains(s)))
            .collect();
        candidates.sort_by_key(|a| a.workload);
        candidates.first().map(|a| a.id.clone())
    }

    pub fn assign_task(&self, task_id: &str, agent_id: &str) -> bool {
        let agent_type = {
            let agents = self.agents.lock().unwrap();
            agents.get(agent_id).map(|a| a.agent_type.clone()).unwrap_or_default()
        };
        if !self.resource_manager.can_accept_task(&agent_type) {
            self.log_message("resource_manager", Some(agent_id), MessageType::Coordination, serde_json::json!({
                "action": "task_throttled", "task_id": task_id, "agent_id": agent_id, "reason": "system_resources_exceeded"
            }));
            return false;
        }
        let task_desc = {
            let mut tasks = self.tasks.lock().unwrap();
            if let Some(task) = tasks.get_mut(task_id) {
                task.status = TaskStatus::Assigned;
                task.assigned_to = Some(agent_id.to_string());
                Some(task.description.clone())
            } else { None }
        };
        if let Some(desc) = task_desc {
            let mut agents = self.agents.lock().unwrap();
            if let Some(agent) = agents.get_mut(agent_id) {
                agent.status = AgentStatus::Busy;
                agent.current_task = Some(task_id.to_string());
                agent.workload += 1;
                self.resource_manager.update_agent_task_count(agent_id, agent.workload as usize);
            }
            drop(agents);
            self.log_message("orchestrator", Some(agent_id), MessageType::TaskResponse, serde_json::json!({
                "action": "task_assigned", "task_id": task_id, "description": desc,
                "priority": self.tasks.lock().unwrap().get(task_id).map(|t| t.priority).unwrap_or(0)
            }));
            true
        } else { false }
    }

    pub fn complete_task(&self, task_id: &str, success: bool) {
        let agent_id = {
            let mut tasks = self.tasks.lock().unwrap();
            if let Some(task) = tasks.get_mut(task_id) {
                task.status = if success { TaskStatus::Completed } else { TaskStatus::Failed };
                task.assigned_to.clone()
            } else { None }
        };
        if let Some(aid) = agent_id {
            let mut agents = self.agents.lock().unwrap();
            if let Some(agent) = agents.get_mut(&aid) {
                agent.status = AgentStatus::Idle;
                agent.current_task = None;
                agent.workload = agent.workload.saturating_sub(1);
                self.resource_manager.update_agent_task_count(&aid, agent.workload as usize);
            }
        }
    }

    pub fn delegate_task(&self, task_id: &str, from_agent: &str) -> Option<String> {
        let task = { self.tasks.lock().unwrap().get(task_id).cloned() }?;
        if let Some(best_agent) = self.find_best_agent(&task.required_skills) {
            if best_agent != from_agent {
                self.assign_task(task_id, &best_agent);
                let context = self.get_agent_context(from_agent);
                self.log_message(from_agent, Some(&best_agent), MessageType::Delegation, serde_json::json!({
                    "task_id": task_id, "description": task.description,
                    "parent_context": context, "delegated_by": from_agent,
                }));
                return Some(best_agent);
            }
        }
        None
    }

    /// Delegate to a specific sub-orchestrator (which will further delegate within its hierarchy)
    pub fn delegate_to_sub_orchestrator(&self, task_id: &str, sub_orch_id: &str, from_agent: &str) -> bool {
        let agents = self.agents.lock().unwrap();
        if !agents.get(sub_orch_id).map(|a| a.is_sub_orchestrator).unwrap_or(false) {
            return false;
        }
        drop(agents);
        let desc = { self.tasks.lock().unwrap().get(task_id).map(|t| t.description.clone()) };
        if let Some(ref d) = desc {
            self.assign_task(task_id, sub_orch_id);
            self.log_message(from_agent, Some(sub_orch_id), MessageType::Delegation, serde_json::json!({
                "task_id": task_id, "description": d, "type": "sub_orchestrator_delegation", "delegated_by": from_agent,
            }));
            true
        } else { false }
    }

    pub fn get_agent_context(&self, agent_id: &str) -> Vec<serde_json::Value> {
        self.messages.lock().unwrap().iter()
            .filter(|m| m.from == agent_id || m.to.as_deref() == Some(agent_id))
            .rev().take(20)
            .map(|m| serde_json::json!({
                "from": m.from, "type": format!("{:?}", m.message_type),
                "summary": format!("{:?}", m.payload).chars().take(200).collect::<String>(),
            }))
            .collect()
    }

    pub fn send_message(&self, from: &str, to: Option<&str>, msg_type: MessageType, payload: serde_json::Value) -> String {
        let id = format!("msg-{}", Uuid::new_v4().to_hex_string());
        let msg = OrchestratorMessage {
            id: id.clone(), from: from.to_string(), to: to.map(String::from),
            message_type: msg_type, payload, timestamp: timestamp_now(),
        };
        self.messages.lock().unwrap().push(msg);
        id
    }

    pub fn get_agent_messages(&self, agent_id: &str, since: i64) -> Vec<OrchestratorMessage> {
        self.messages.lock().unwrap().iter()
            .filter(|m| m.timestamp > since && (m.to.as_deref() == Some(agent_id) || m.to.is_none()))
            .cloned().collect()
    }

    pub fn get_idle_agents(&self) -> Vec<Agent> {
        self.agents.lock().unwrap().values()
            .filter(|a| a.status == AgentStatus::Idle).cloned().collect()
    }

    pub fn get_pending_tasks(&self) -> Vec<Task> {
        self.tasks.lock().unwrap().values()
            .filter(|t| t.status == TaskStatus::Pending).cloned().collect()
    }

    pub fn get_system_status(&self) -> serde_json::Value {
        let agents = self.agents.lock().unwrap();
        let tasks = self.tasks.lock().unwrap();
        let files = self.legacy_files.lock().unwrap();
        let sub_orchs: Vec<&Agent> = agents.values().filter(|a| a.is_sub_orchestrator).collect();
        serde_json::json!({
            "agents": {
                "total": agents.len(),
                "idle": agents.values().filter(|a| a.status == AgentStatus::Idle).count(),
                "busy": agents.values().filter(|a| a.status == AgentStatus::Busy).count(),
                "sub_orchestrators": sub_orchs.len(),
                "list": agents.values().map(|a| serde_json::json!({
                    "id": a.id, "type": a.agent_type, "status": format!("{:?}", a.status),
                    "skills": a.skills, "workload": a.workload,
                    "is_sub_orch": a.is_sub_orchestrator,
                    "parent": a.parent_agent,
                    "sub_agents": a.sub_agents,
                })).collect::<Vec<_>>()
            },
            "tasks": {
                "pending": tasks.values().filter(|t| t.status == TaskStatus::Pending).count(),
                "in_progress": tasks.values().filter(|t| t.status == TaskStatus::InProgress).count(),
                "completed": tasks.values().filter(|t| t.status == TaskStatus::Completed).count(),
                "awaiting_review": tasks.values().filter(|t| t.status == TaskStatus::AwaitingReview).count(),
                "coordinated": tasks.values().filter(|t| t.coordinated).count(),
            },
            "legacy_files": files.len(),
            "timestamp": timestamp_now()
        })
    }

    pub fn log_message(&self, from: &str, to: Option<&str>, msg_type: MessageType, payload: serde_json::Value) {
        self.send_message(from, to, msg_type, payload);
    }

    pub fn cleanup_stale_agents(&self, timeout_secs: u64) {
        let now = timestamp_now();
        let timeout = timeout_secs as i64;
        let stale: Vec<String> = self.agents.lock().unwrap().iter()
            .filter(|(_, a)| now - a.last_heartbeat > timeout)
            .map(|(id, _)| id.clone()).collect();
        for id in stale { self.unregister_agent(&id); }
    }
}

fn timestamp_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}
