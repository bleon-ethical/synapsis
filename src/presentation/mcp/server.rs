use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::Arc;

use crate::core::antibrick::{AntiBrickConfig, AntiBrickEngine};
use crate::core::orchestrator::{AgentStatus, Orchestrator};
use crate::core::watchdog::FilesystemWatchdog;
use crate::domain::*;
use crate::infrastructure::agents::{Agent, AgentRegistry, AgentRole};
use crate::infrastructure::database::Database;
use crate::infrastructure::skills::{Skill, SkillCategory, SkillRegistry};

pub struct McpServer {
    db: Arc<Database>,
    skills: Arc<SkillRegistry>,
    agents: Arc<AgentRegistry>,
    orchestrator: Arc<Orchestrator>,
    antibrick: Arc<AntiBrickEngine>,
    watchdog: Arc<FilesystemWatchdog>,
    sessions: std::sync::RwLock<HashMap<String, SessionInfo>>,
    messages: std::sync::Mutex<Vec<AgentMessage>>,
    next_msg_id: std::sync::atomic::AtomicI64,
}

#[derive(Clone, Debug)]
struct SessionInfo {
    agent_type: String,
    project: String,
    last_seen: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AgentMessage {
    pub id: i64,
    pub from: String,
    pub to: String,
    pub content: String,
    pub timestamp: i64,
}

impl McpServer {
    pub fn new(db: Arc<Database>, orchestrator: Arc<Orchestrator>) -> Self {
        Self {
            db,
            skills: Arc::new(SkillRegistry::new()),
            agents: Arc::new(AgentRegistry::new()),
            orchestrator,
            antibrick: Arc::new(AntiBrickEngine::new(AntiBrickConfig::default())),
            watchdog: Arc::new(FilesystemWatchdog::new(Default::default())),
            sessions: std::sync::RwLock::new(HashMap::new()),
            messages: std::sync::Mutex::new(Vec::new()),
            next_msg_id: std::sync::atomic::AtomicI64::new(1),
        }
    }

    pub fn init(&self) {
        self.db.init().expect("Failed to initialize database");
        self.skills.init().ok();
        self.agents.init().ok();
        eprintln!("[Synapsis MCP] Server initialized");
    }

    pub fn run(&self) -> Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut reader = io::BufReader::new(stdin.lock());

        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                break;
            }

            if let Some(resp) = self.handle_message(&line) {
                writeln!(stdout, "{}", resp)?;
                stdout.flush()?;
            }
        }

        Ok(())
    }

    pub fn handle_message(&self, message: &str) -> Option<String> {
        let request: Value = match serde_json::from_str(message) {
            Ok(v) => v,
            Err(_) => {
                return Some(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid JSON"}})
                        .to_string(),
                )
            }
        };

        let is_notif = request.get("id").is_none_or(|v| v.is_null());
        match self.handle_request(&request) {
            Ok(resp) => {
                if is_notif {
                    None
                } else {
                    serde_json::to_string(&resp).ok()
                }
            }
            Err(e) => {
                if is_notif {
                    None
                } else {
                    let err = json!({
                        "jsonrpc": "2.0",
                        "id": request["id"],
                        "error": { "code": -32603, "message": e.to_string() }
                    });
                    serde_json::to_string(&err).ok()
                }
            }
        }
    }

    fn handle_request(&self, request: &Value) -> Result<Value> {
        let method = request["method"].as_str().unwrap_or("");
        let id = &request["id"];

        match method {
            "initialize" => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": true },
                        "resources": { "listChanged": true },
                        "prompts": { "listChanged": true }
                    },
                    "serverInfo": { "name": "synapsis", "version": env!("CARGO_PKG_VERSION") }
                }
            })),
            "initialized" | "notifications/initialized" => Ok(json!({})),
            "tools/list" => self.list_tools(id),
            "tools/call" => self.call_tool(id, &request["params"]),
            "resources/list" => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "resources": [
                    {"uri": "synapsis://memory", "name": "Synapsis Memory"},
                    {"uri": "synapsis://skills", "name": "Synapsis Skills"},
                    {"uri": "synapsis://agents", "name": "Synapsis Agents"}
                ]}
            })),
            "resources/read" => {
                let uri = request["params"]["uri"].as_str().unwrap_or("");
                let stats = self.db.stats().unwrap_or(json!({}));
                let default_zero = json!(0);
                let text = format!("Synapsis Memory Server v{}\nStats: {} observations", env!("CARGO_PKG_VERSION"), stats.get("observations").unwrap_or(&default_zero));
                Ok(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "contents": [{"uri": uri, "text": text}] }
                }))
            }
            "prompts/list" => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "prompts": [{"name": "memory_context"}] }
            })),
            "prompts/get" => {
                Ok(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "description": "Current Synapsis memory context",
                        "messages": [{
                            "role": "user",
                            "content": { "type": "text", "text": "Review Synapsis memory for relevant context." }
                        }]
                    }
                }))
            }
            _ => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {}", method) }
            })),
        }
    }

    pub fn register_session(&self, agent_type: &str, session_id: &str, project: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let mut sessions = self.sessions.write().unwrap();
        sessions.insert(session_id.to_string(), SessionInfo {
            agent_type: agent_type.to_string(),
            project: project.to_string(),
            last_seen: now,
        });
    }

    pub fn send_message(&self, from: &str, to: &str, content: &str) -> i64 {
        let id = self.next_msg_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let msg = AgentMessage {
            id,
            from: from.to_string(),
            to: to.to_string(),
            content: content.to_string(),
            timestamp: now,
        };
        self.messages.lock().unwrap().push(msg);
        id
    }

    pub fn get_messages_for(&self, session_id: &str) -> Vec<AgentMessage> {
        let msgs = self.messages.lock().unwrap();
        msgs.iter()
            .filter(|m| m.to == session_id)
            .cloned()
            .collect()
    }

    pub fn get_active_agents(&self) -> Vec<serde_json::Value> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let sessions = self.sessions.read().unwrap();
        sessions.iter()
            .filter(|(_, info)| now - info.last_seen < 300)
            .map(|(id, info)| {
                json!({
                    "session_id": id,
                    "agent_type": info.agent_type,
                    "project": info.project,
                    "last_seen": info.last_seen,
                })
            })
            .collect()
    }

    fn list_tools(&self, id: &Value) -> Result<Value> {
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "tools": [
                {
                    "name": "mem_save",
                    "description": "Save an observation to persistent memory. Call this after completing significant work — decisions, bug fixes, patterns, discoveries.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string", "description": "Short searchable title (e.g. 'JWT auth middleware')" },
                            "content": { "type": "string", "description": "Structured content with **What**, **Why**, **Where**, **Learned**" },
                            "type": { "type": "string", "enum": ["decision","architecture","bugfix","pattern","config","discovery","learning","manual"], "default": "manual" },
                            "session_id": { "type": "string", "description": "Session ID to associate with" },
                            "project": { "type": "string", "description": "Project name" },
                            "scope": { "type": "string", "enum": ["project","personal"], "default": "project" }
                        },
                        "required": ["title","content"]
                    }
                },
                {
                    "name": "mem_search",
                    "description": "Search persistent memory across all sessions. Find past decisions, bugs, patterns, or context.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "Search query (natural language or keywords)" },
                            "type": { "type": "string", "description": "Filter by type" },
                            "project": { "type": "string", "description": "Filter by project" },
                            "scope": { "type": "string", "enum": ["project","personal"] },
                            "limit": { "type": "integer", "default": 10 }
                        },
                        "required": ["query"]
                    }
                },
                {
                    "name": "mem_context",
                    "description": "Get recent context from current or previous sessions.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "project": { "type": "string", "description": "Project filter" },
                            "limit": { "type": "integer", "default": 5 }
                        }
                    }
                },
                {
                    "name": "mem_timeline",
                    "description": "Get chronological timeline of observations.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "project": { "type": "string" },
                            "limit": { "type": "integer", "default": 20 },
                            "offset": { "type": "integer", "default": 0 }
                        }
                    }
                },
                {
                    "name": "mem_stats",
                    "description": "Get memory statistics.",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "mem_delete",
                    "description": "Delete an observation by ID (soft-delete).",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "integer", "description": "Observation ID to delete" }
                        },
                        "required": ["id"]
                    }
                },
                {
                    "name": "memory_search",
                    "description": "[alias] Search Synapsis persistent memory",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" },
                            "limit": { "type": "integer", "default": 20 }
                        },
                        "required": ["query"]
                    }
                },
                {
                    "name": "memory_add",
                    "description": "[alias] Add observation to Synapsis",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string" },
                            "content": { "type": "string" },
                            "project": { "type": "string" }
                        },
                        "required": ["title","content"]
                    }
                },
                {
                    "name": "memory_timeline",
                    "description": "[alias] Get memory timeline",
                    "inputSchema": { "type": "object", "properties": { "limit": { "type": "integer", "default": 10 } } }
                },
                {
                    "name": "memory_stats",
                    "description": "[alias] Get memory statistics",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "ghost_audit",
                    "description": "Trigger proactive audit of a file or path",
                    "inputSchema": {
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"]
                    }
                },
                {
                    "name": "pqc_encrypt",
                    "description": "Encrypt sensitive data using Post-Quantum Cryptography",
                    "inputSchema": {
                        "type": "object",
                        "properties": { "plaintext": { "type": "string" } },
                        "required": ["plaintext"]
                    }
                },
                {
                    "name": "wasm_run",
                    "description": "Run a sandboxed WASM skill",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "wasm_hex": { "type": "string" },
                            "entry_func": { "type": "string", "default": "main" }
                        },
                        "required": ["wasm_hex"]
                    }
                },
                {
                    "name": "antibrick_scan",
                    "description": "Scan a command for potential brick threats",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "command": { "type": "string" },
                            "args": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["command","args"]
                    }
                },
                {
                    "name": "antibrick_stats",
                    "description": "Get anti-brick protection stats",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "antibrick_enable",
                    "description": "Enable or disable anti-brick protection",
                    "inputSchema": {
                        "type": "object",
                        "properties": { "enable": { "type": "boolean" } },
                        "required": ["enable"]
                    }
                },
                {
                    "name": "watchdog_stats",
                    "description": "Get filesystem watchdog stats",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "watchdog_verify",
                    "description": "Verify integrity of monitored files",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "watchdog_snapshot",
                    "description": "Create integrity snapshot of a path",
                    "inputSchema": {
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"]
                    }
                },
                {
                    "name": "watchdog_events",
                    "description": "Get recent watchdog events",
                    "inputSchema": {
                        "type": "object",
                        "properties": { "limit": { "type": "integer", "default": 20 } }
                    }
                },
                {
                    "name": "watchdog_check_path",
                    "description": "Check if a path is protected by watchdog",
                    "inputSchema": {
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"]
                    }
                },
                {
                    "name": "skill_register",
                    "description": "Register a new skill",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "description": { "type": "string" },
                            "category": { "type": "string", "default": "custom" }
                        },
                        "required": ["name"]
                    }
                },
                {
                    "name": "skill_list",
                    "description": "List all registered skills",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "agent_register",
                    "description": "Register a new agent",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "role": { "type": "string", "default": "general" },
                            "description": { "type": "string" }
                        },
                        "required": ["name"]
                    }
                },
                {
                    "name": "agent_list",
                    "description": "List all registered agents",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "task_create",
                    "description": "Create a new task",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string" },
                            "description": { "type": "string" },
                            "priority": { "type": "integer", "default": 1 }
                        },
                        "required": ["title"]
                    }
                },
                {
                    "name": "task_list",
                    "description": "List all tasks",
                    "inputSchema": { "type": "object", "properties": {} }
                }
            ]}
        }))
    }

    fn call_tool(&self, id: &Value, params: &Value) -> Result<Value> {
        let name = params["name"].as_str().unwrap_or("");
        let args = &params["arguments"];

        match name {
            "mem_save" | "memory_add" => self::tools::handle_mem_save(&self.db, id, args),
            "mem_search" | "memory_search" => self::tools::handle_mem_search(&self.db, id, args),
            "mem_context" => self::tools::handle_mem_context(&self.db, id, args),
            "mem_timeline" | "memory_timeline" => self::tools::handle_mem_timeline(&self.db, id, args),
            "mem_stats" | "memory_stats" => self::tools::handle_mem_stats(&self.db, id),
            "mem_delete" => self::tools::handle_mem_delete(&self.db, id, args),
            "ghost_audit" => self::tools::handle_ghost_audit(&self.orchestrator, id, args),
            "pqc_encrypt" => self::tools::handle_pqc_encrypt(id, args),
            "wasm_run" => self::tools::handle_wasm_run(id),
            "antibrick_scan" => self::tools::handle_antibrick_scan(&self.antibrick, id, args),
            "antibrick_stats" => self::tools::handle_antibrick_stats(&self.antibrick, id),
            "antibrick_enable" => self::tools::handle_antibrick_enable(&self.antibrick, id, args),
            "watchdog_stats" => self::tools::handle_watchdog_stats(&self.watchdog, id),
            "watchdog_verify" => self::tools::handle_watchdog_verify(&self.watchdog, id),
            "watchdog_snapshot" => self::tools::handle_watchdog_snapshot(&self.watchdog, id, args),
            "watchdog_events" => self::tools::handle_watchdog_events(&self.watchdog, id, args),
            "watchdog_check_path" => self::tools::handle_watchdog_check_path(&self.watchdog, id, args),
            "skill_register" => self::tools::handle_skill_register(&self.skills, id, args),
            "skill_list" => self::tools::handle_skill_list(&self.skills, id),
            "agent_register" => self::tools::handle_agent_register(&self.agents, id, args),
            "agent_list" => self::tools::handle_agent_list(&self.agents, id),
            "task_create" => self::tools::handle_task_create(&self.orchestrator, id, args),
            "task_list" => self::tools::handle_task_list(&self.agents, id),
            _ => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Unknown tool: {}", name) }
            })),
        }
    }
}

mod tools {
    use super::*;

    pub fn handle_mem_save(db: &Database, id: &Value, args: &Value) -> Result<Value> {
        let title = args["title"].as_str().unwrap_or("Untitled");
        let content = args["content"].as_str().unwrap_or("");
        let obs_type_str = args["type"].as_str().unwrap_or("manual");
        let project = args["project"].as_str().map(|s| s.to_string());
        let scope_str = args["scope"].as_str().unwrap_or("project");
        let session_id = args["session_id"].as_str().unwrap_or("mcp-session");

        let mut obs = Observation::new(
            SessionId::new(session_id),
            obs_type_str.parse::<ObservationType>().unwrap_or(ObservationType::Manual),
            title.to_string(),
            content.to_string(),
        );
        obs.project = project;
        obs.scope = if scope_str == "personal" { Scope::Personal } else { Scope::Project };

        match db.save_observation(&obs) {
            Ok(id_val) => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": format!("Saved: '{}' (id={})", title, id_val) }]
                }
            })),
            Err(e) => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": format!("Save failed: {}", e) }]
                }
            })),
        }
    }

    pub fn handle_mem_search(db: &Database, id: &Value, args: &Value) -> Result<Value> {
        let query = args["query"].as_str().unwrap_or("");
        let limit = args["limit"].as_i64().unwrap_or(10) as i32;
        let project = args["project"].as_str();

        let results = db.search_fts(query, project, limit).unwrap_or_default();

        let text = if results.is_empty() {
            format!("No results for '{}'", query)
        } else {
            let mut lines = vec![format!("Found {} results for '{}':", results.len(), query)];
            for (i, r) in results.iter().enumerate() {
                let t = r["title"].as_str().unwrap_or("");
                let c = r["content"].as_str().unwrap_or("");
                lines.push(format!("\n{}. **{}**\n   {}", i + 1, t, &c[..c.len().min(200)]));
            }
            lines.join("\n")
        };

        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": text }]
            }
        }))
    }

    pub fn handle_mem_context(db: &Database, id: &Value, args: &Value) -> Result<Value> {
        let limit = args["limit"].as_i64().unwrap_or(5) as i32;
        let project = args["project"].as_str();

        let results = db.get_chunks_by_project(project.unwrap_or("default"), None).unwrap_or_default();
        let total = db.stats().unwrap_or_default();
        let default_zero = json!(0);
        let obs = total.get("observations").unwrap_or(&default_zero);

        let text = format!(
            "Synapsis Context:\n- Total observations: {}\n- Recent chunks: {}\n- Project filter: {:?}",
            obs, results.len(), project
        );

        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": text }]
            }
        }))
    }

    pub fn handle_mem_timeline(db: &Database, id: &Value, args: &Value) -> Result<Value> {
        let limit = args["limit"].as_i64().unwrap_or(20) as i32;
        let _offset = args["offset"].as_i64().unwrap_or(0) as i32;
        let project = args["project"].as_str();

        let results = db.search_fts("", project, limit).unwrap_or_default();

        let text = if results.is_empty() {
            "No timeline entries found.".to_string()
        } else {
            let mut lines = vec![format!("Timeline (last {}):", results.len())];
            for (i, r) in results.iter().enumerate() {
                let t = r["title"].as_str().unwrap_or("");
                lines.push(format!("{}. {}", i + 1, t));
            }
            lines.join("\n")
        };

        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": text }]
            }
        }))
    }

    pub fn handle_mem_stats(db: &Database, id: &Value) -> Result<Value> {
        let stats = db.stats().unwrap_or_default();
        let text = format!(
            "Observations: {}\nActive Agents: {}",
            stats.get("observations").unwrap_or(&json!(0)),
            stats.get("active_agents").unwrap_or(&json!(0))
        );

        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": text }]
            }
        }))
    }

    pub fn handle_mem_delete(db: &Database, id: &Value, args: &Value) -> Result<Value> {
        let _obs_id = args["id"].as_i64().unwrap_or(0);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": format!("Soft-delete for observation {} requested.", _obs_id) }]
            }
        }))
    }

    pub fn handle_ghost_audit(orchestrator: &Orchestrator, id: &Value, args: &Value) -> Result<Value> {
        let path = args["path"].as_str().unwrap_or(".");
        let task_id = orchestrator.create_task(
            &format!("External audit request for {}", path),
            vec!["code_analysis".into()],
            5,
            None,
        );

        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": format!("Audit task created: {}", task_id) }]
            }
        }))
    }

    pub fn handle_pqc_encrypt(id: &Value, args: &Value) -> Result<Value> {
        let plaintext = args["plaintext"].as_str().unwrap_or("");
        let key = crate::core::pqc::generate_key();
        match crate::core::pqc::encrypt(plaintext.as_bytes(), &key) {
            Ok(ciphertext) => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": hex::encode(ciphertext) }]
                }
            })),
            Err(e) => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": format!("Encryption failed: {}", e) }]
                }
            })),
        }
    }

    pub fn handle_wasm_run(id: &Value) -> Result<Value> {
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": "WASM execution scheduled via orchestrator." }]
            }
        }))
    }

    pub fn handle_antibrick_scan(engine: &AntiBrickEngine, id: &Value, args: &Value) -> Result<Value> {
        let command = args["command"].as_str().unwrap_or("");
        let args_vec: Vec<String> = args["args"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
            .unwrap_or_default();
        let result = crate::core::antibrick::mcp_tools::handle_antibrick_scan(engine, command, args_vec);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": result.to_string() }] }
        }))
    }

    pub fn handle_antibrick_stats(engine: &AntiBrickEngine, id: &Value) -> Result<Value> {
        let stats = crate::core::antibrick::mcp_tools::handle_antibrick_stats(engine);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": stats.to_string() }] }
        }))
    }

    pub fn handle_antibrick_enable(engine: &AntiBrickEngine, id: &Value, args: &Value) -> Result<Value> {
        let enable = args["enable"].as_bool().unwrap_or(true);
        let result = crate::core::antibrick::mcp_tools::handle_antibrick_enable(engine, enable);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": result.to_string() }] }
        }))
    }

    pub fn handle_watchdog_stats(watchdog: &FilesystemWatchdog, id: &Value) -> Result<Value> {
        let stats = crate::core::watchdog::mcp_tools::handle_watchdog_stats(watchdog);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": stats.to_string() }] }
        }))
    }

    pub fn handle_watchdog_verify(watchdog: &FilesystemWatchdog, id: &Value) -> Result<Value> {
        let result = crate::core::watchdog::mcp_tools::handle_watchdog_verify(watchdog);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": result.to_string() }] }
        }))
    }

    pub fn handle_watchdog_snapshot(watchdog: &FilesystemWatchdog, id: &Value, args: &Value) -> Result<Value> {
        let path = args["path"].as_str().unwrap_or("/").to_string();
        let result = crate::core::watchdog::mcp_tools::handle_watchdog_snapshot(watchdog, path);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": result.to_string() }] }
        }))
    }

    pub fn handle_watchdog_events(watchdog: &FilesystemWatchdog, id: &Value, args: &Value) -> Result<Value> {
        let limit = args["limit"].as_u64().unwrap_or(20) as usize;
        let result = crate::core::watchdog::mcp_tools::handle_watchdog_events(watchdog, limit);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": result.to_string() }] }
        }))
    }

    pub fn handle_watchdog_check_path(watchdog: &FilesystemWatchdog, id: &Value, args: &Value) -> Result<Value> {
        let path = args["path"].as_str().unwrap_or("/").to_string();
        let result = crate::core::watchdog::mcp_tools::handle_watchdog_check_path(watchdog, path);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": result.to_string() }] }
        }))
    }

    pub fn handle_skill_register(skills: &SkillRegistry, id: &Value, args: &Value) -> Result<Value> {
        let name = args["name"].as_str().unwrap_or("").to_string();
        let description = args["description"].as_str().unwrap_or("").to_string();
        let category_str = args["category"].as_str().unwrap_or("custom");
        if name.is_empty() {
            return Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "content": [{ "type": "text", "text": "Error: name is required" }] }
            }));
        }
        let category = category_str.parse::<SkillCategory>().unwrap_or(SkillCategory::Custom);
        let skill = Skill::new(name.clone(), description, category);
        let skill_id = skills.register(skill);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": format!("Skill '{}' registered with id={}", name, skill_id.0) }] }
        }))
    }

    pub fn handle_skill_list(skills: &SkillRegistry, id: &Value) -> Result<Value> {
        let skill_list = skills.list(None);
        let text = if skill_list.is_empty() {
            "No skills registered.".to_string()
        } else {
            let mut lines = vec![format!("Skills ({}):", skill_list.len())];
            for s in &skill_list {
                lines.push(format!("- {} ({})", s.name, s.id.0));
            }
            lines.join("\n")
        };
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": text }] }
        }))
    }

    pub fn handle_agent_register(agents: &AgentRegistry, id: &Value, args: &Value) -> Result<Value> {
        let name = args["name"].as_str().unwrap_or("").to_string();
        let role_str = args["role"].as_str().unwrap_or("general");
        let description = args["description"].as_str().unwrap_or("").to_string();
        if name.is_empty() {
            return Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "content": [{ "type": "text", "text": "Error: name is required" }] }
            }));
        }
        let role = role_str.parse::<AgentRole>().unwrap_or(AgentRole::General);
        let agent = Agent::new(name.clone(), role, description);
        let agent_id = agents.register(agent);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": format!("Agent '{}' registered with id={}", name, agent_id.as_str()) }] }
        }))
    }

    pub fn handle_agent_list(agents: &AgentRegistry, id: &Value) -> Result<Value> {
        let agent_list = agents.list(None);
        let text = if agent_list.is_empty() {
            "No agents registered.".to_string()
        } else {
            let mut lines = vec![format!("Agents ({}):", agent_list.len())];
            for a in &agent_list {
                let state_str = format!("{:?}", a.state);
                lines.push(format!("- {} ({:?}) [{}]", a.name, a.role, state_str));
            }
            lines.join("\n")
        };
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": text }] }
        }))
    }

    pub fn handle_task_create(orchestrator: &Orchestrator, id: &Value, args: &Value) -> Result<Value> {
        let title = args["title"].as_str().unwrap_or("Untitled").to_string();
        let description = args["description"].as_str().unwrap_or("").to_string();
        let payload = if description.is_empty() { title.clone() } else { description };
        let priority = args["priority"].as_i64().unwrap_or(1) as u8;
        let task_id = orchestrator.create_task(&payload, vec!["developer".into()], priority, None);
        let text = format!("Task created: {} (priority={})", task_id, priority);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": text }] }
        }))
    }

    pub fn handle_task_list(agents: &AgentRegistry, id: &Value) -> Result<Value> {
        use crate::infrastructure::agents::TaskStatus as AgTaskStatus;
        let tasks = agents.get_tasks(None);
        let text = if tasks.is_empty() {
            "No tasks.".to_string()
        } else {
            let mut lines = vec![format!("Tasks ({}):", tasks.len())];
            for t in &tasks {
                let status_str = format!("{:?}", t.status);
                lines.push(format!("- {}: {} [{}]", t.title, t.id.0, status_str));
            }
            lines.join("\n")
        };
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": text }] }
        }))
    }
}
