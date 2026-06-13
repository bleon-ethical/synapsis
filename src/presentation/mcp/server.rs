use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

macro_rules! info_log {
    ($($arg:tt)*) => {{
        let quiet = std::env::var("SYNAPSIS_QUIET").is_ok() || std::env::var("QUIET").is_ok();
        if !quiet {
            eprintln!($($arg)*);
        }
    }};
}

macro_rules! debug_log {
    ($($arg:tt)*) => {{
        if std::env::var("SYNAPSIS_LOG").as_deref() == Ok("debug") {
            eprintln!($($arg)*);
        }
    }};
}

use crate::core::antibrick::{AntiBrickConfig, AntiBrickEngine};
use crate::core::orchestrator::Orchestrator;
use crate::core::watchdog::FilesystemWatchdog;
use crate::domain::*;
use crate::infrastructure::agents::{Agent, AgentRegistry, AgentRole};
use crate::infrastructure::database::Database;
use crate::infrastructure::skills::{Skill, SkillCategory, SkillRegistry};

static MCP_CALL_ID: AtomicU64 = AtomicU64::new(1);

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
        info_log!("[Synapsis MCP] Server initialized");
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

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(resp) = self.handle_message(trimmed) {
                writeln!(stdout, "{}", resp)?;
                stdout.flush()?;
            }
        }

        Ok(())
    }

    pub fn handle_message(&self, message: &str) -> Option<String> {
        let request: Value = match serde_json::from_str(message) {
            Ok(v) => v,
            Err(e) => {
                debug_log!("[Synapsis MCP] Parse error: {} | Message: '{}'", e, message);
                return Some(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid JSON"}})
                        .to_string(),
                );
            }
        };

        let is_tool_call = request["method"].as_str() == Some("tools/call");
        let tool_name = request["params"]["name"].as_str().unwrap_or("").to_string();
        let is_notif = request.get("id").is_none_or(|v| v.is_null());
        let result = self.handle_request(&request);

        match result {
            Ok(resp) => {
                let has_error = resp.get("error").is_some();
                if is_tool_call && !has_error && !tool_name.is_empty() && tool_name != "mem_save" {
                    self.auto_save_observation(&tool_name, &request["params"]["arguments"]);
                }
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

    fn auto_save_observation(&self, tool_name: &str, args: &Value) {
        let content = format_args_snapshot(tool_name, args);
        let title = format!("tool:{}", tool_name);
        let mut obs = crate::domain::Observation::new(
            crate::domain::SessionId::new("mcp-auto"),
            crate::domain::ObservationType::Manual,
            title,
            content,
        );
        obs.project = Some("synapsis".to_string());
        obs.scope = crate::domain::Scope::Project;
        self.db.save_observation(&obs).ok();
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
                let text = format!(
                    "Synapsis Memory Server v{}\nStats: {} observations",
                    env!("CARGO_PKG_VERSION"),
                    stats.get("observations").unwrap_or(&default_zero)
                );
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
            "prompts/get" => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "description": "Current Synapsis memory context",
                    "messages": [{
                        "role": "user",
                        "content": { "type": "text", "text": "Review Synapsis memory for relevant context." }
                    }]
                }
            })),
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
            .unwrap_or_default()
            .as_secs() as i64;
        let mut sessions = self.sessions.write().unwrap();
        sessions.insert(
            session_id.to_string(),
            SessionInfo {
                agent_type: agent_type.to_string(),
                project: project.to_string(),
                last_seen: now,
            },
        );
    }

    pub fn send_message(&self, from: &str, to: &str, content: &str) -> i64 {
        let id = self
            .next_msg_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
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
            .unwrap_or_default()
            .as_secs() as i64;
        let sessions = self.sessions.read().unwrap();
        sessions
            .iter()
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
                },
                {
                    "name": "mcp_call",
                    "description": "Call a tool on another MCP server. Allows sub-orchestrators and agents to dispatch to any MCP tool via HTTP.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "server_url": { "type": "string", "description": "Base URL of target MCP server" },
                            "tool_name": { "type": "string", "description": "Name of the tool to call" },
                            "arguments": { "type": "object", "description": "Tool arguments", "default": {} },
                            "endpoint": { "type": "string", "description": "Custom endpoint path", "default": "/message" }
                        },
                        "required": ["server_url", "tool_name"]
                    }
                },
                {
                    "name": "browser_navigate",
                    "description": "Fetch a web page as an HTTP client. Extracts title, text content, links, and metadata. For dataset generation, review, and M.A.T.E.R.I.A. training data collection.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "url": { "type": "string", "description": "URL to fetch" },
                            "method": { "type": "string", "enum": ["GET", "POST"], "default": "GET" },
                            "headers": { "type": "object", "description": "Custom HTTP headers", "default": {} },
                            "body": { "type": "string", "description": "POST body (if method=POST)" },
                            "extract": { "type": "string", "enum": ["full", "text", "links", "meta"], "default": "full", "description": "What to extract from the page" }
                        },
                        "required": ["url"]
                    }
                },
                {
                    "name": "browser_snapshot",
                    "description": "Get a structured snapshot of a web page: title, description, headings, links count, text preview.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "url": { "type": "string" },
                            "max_text": { "type": "integer", "default": 2000, "description": "Max chars of text content" }
                        },
                        "required": ["url"]
                    }
                },
                {
                    "name": "db_backup",
                    "description": "Create a backup of the Synapsis database.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Destination path for the backup file" }
                        },
                        "required": ["path"]
                    }
                },
                {
                    "name": "db_integrity",
                    "description": "Run PRAGMA integrity_check on the database.",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "db_prune",
                    "description": "Soft-delete observations older than N days.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "older_than_days": { "type": "integer", "default": 90, "description": "Delete observations older than this many days" }
                        }
                    }
                },
                {
                    "name": "db_vacuum",
                    "description": "Reclaim unused space in the database (VACUUM).",
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
            "mem_timeline" | "memory_timeline" => {
                self::tools::handle_mem_timeline(&self.db, id, args)
            }
            "mem_stats" | "memory_stats" => self::tools::handle_mem_stats(&self.db, id),
            "mem_delete" => self::tools::handle_mem_delete(&self.db, id, args),
            "ghost_audit" => self::tools::handle_ghost_audit(&self.orchestrator, id, args),
            "pqc_encrypt" => self::tools::handle_pqc_encrypt(id, args),
            "wasm_run" => self::tools::handle_wasm_run(id, args),
            "antibrick_scan" => self::tools::handle_antibrick_scan(&self.antibrick, id, args),
            "antibrick_stats" => self::tools::handle_antibrick_stats(&self.antibrick, id),
            "antibrick_enable" => self::tools::handle_antibrick_enable(&self.antibrick, id, args),
            "watchdog_stats" => self::tools::handle_watchdog_stats(&self.watchdog, id),
            "watchdog_verify" => self::tools::handle_watchdog_verify(&self.watchdog, id),
            "watchdog_snapshot" => self::tools::handle_watchdog_snapshot(&self.watchdog, id, args),
            "watchdog_events" => self::tools::handle_watchdog_events(&self.watchdog, id, args),
            "watchdog_check_path" => {
                self::tools::handle_watchdog_check_path(&self.watchdog, id, args)
            }
            "db_backup" => self::tools::handle_db_backup(&self.db, id, args),
            "db_integrity" => self::tools::handle_db_integrity(&self.db, id),
            "db_prune" => self::tools::handle_db_prune(&self.db, id, args),
            "db_vacuum" => self::tools::handle_db_vacuum(&self.db, id),
            "skill_register" => self::tools::handle_skill_register(&self.skills, id, args),
            "skill_list" => self::tools::handle_skill_list(&self.skills, id),
            "agent_register" => self::tools::handle_agent_register(&self.agents, id, args),
            "agent_list" => self::tools::handle_agent_list(&self.agents, id),
            "task_create" => self::tools::handle_task_create(&self.orchestrator, id, args),
            "task_list" => self::tools::handle_task_list(&self.orchestrator, id),
            "mcp_call" => self::tools::handle_mcp_call(id, args),
            "browser_navigate" => self::tools::handle_browser_navigate(id, args),
            "browser_snapshot" => self::tools::handle_browser_snapshot(id, args),
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
            obs_type_str
                .parse::<ObservationType>()
                .unwrap_or(ObservationType::Manual),
            title.to_string(),
            content.to_string(),
        );
        obs.project = project;
        obs.scope = if scope_str == "personal" {
            Scope::Personal
        } else {
            Scope::Project
        };

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

        // Use FTS5 for non-empty queries, fall back to LIKE for empty/project-only
        let results = if !query.is_empty() && !query.contains('%') && !query.contains('_') {
            db.search_fts5(query, limit).unwrap_or_default()
        } else {
            let project = args["project"].as_str();
            db.search_fts(query, project, limit).unwrap_or_default()
        };

        let text = if results.is_empty() {
            format!("No results for '{}'", query)
        } else {
            let mut lines = vec![format!("Found {} results for '{}':", results.len(), query)];
            for (i, r) in results.iter().enumerate() {
                let t = r["title"].as_str().unwrap_or("");
                let c = r["content"].as_str().unwrap_or("");
                let preview: String = c.chars().take(200).collect();
                lines.push(format!(
                    "\n{}. **{}**\n   {}",
                    i + 1,
                    t,
                    preview
                ));
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
        let _limit = args["limit"].as_i64().unwrap_or(5) as i32;
        let project = args["project"].as_str();

        let results = db
            .get_chunks_by_project(project.unwrap_or("default"), None)
            .unwrap_or_default();
        let total = db.stats().unwrap_or_default();
        let default_zero = json!(0);
        let obs = total.get("observations").unwrap_or(&default_zero);

        let text = format!(
            "Context\n- Observations: {}\n- Recent chunks: {}\n- Project: {}",
            obs, results.len(), project.unwrap_or("(all)")
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

        let results = db.get_timeline_direct(limit).unwrap_or_default();

        let text = if results.is_empty() {
            "No timeline entries found.".to_string()
        } else {
            let mut lines = vec![format!("Timeline (last {}):", results.len())];
            for (i, entry) in results.iter().enumerate() {
                let t = entry.observation.title.as_str();
                let ts = entry.observation.created_at.0;
                lines.push(format!("{}. {} ({})", i + 1, t, ts));
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
        let obs_id = args["id"].as_i64().unwrap_or(0);
        if obs_id == 0 {
            return Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": "Missing or invalid observation id" }
            }));
        }
        match db.soft_delete_observation(obs_id) {
            Ok(_) => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": format!("Observation {} soft-deleted.", obs_id) }]
                }
            })),
            Err(e) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "content": [{ "type": "text", "text": format!("Delete failed: {}", e) }] }
            })),
        }
    }

    pub fn handle_ghost_audit(
        orchestrator: &Orchestrator,
        id: &Value,
        args: &Value,
    ) -> Result<Value> {
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
        let key = derive_encryption_key();
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

    pub fn handle_wasm_run(id: &Value, args: &Value) -> Result<Value> {
        let wasm_hex = args["wasm_hex"].as_str().unwrap_or("").to_string();
        let entry_func = args["entry_func"].as_str().unwrap_or("main").to_string();

        if wasm_hex.is_empty() {
            return Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": "Missing required parameter: wasm_hex" }
            }));
        }

        let wasm_size = wasm_hex.len() / 2;
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": format!(
                    "WASM module received ({} bytes, entry={}). Execution not yet implemented in this build.",
                    wasm_size, entry_func
                )}]
            }
        }))
    }

    pub fn handle_antibrick_scan(
        engine: &AntiBrickEngine,
        id: &Value,
        args: &Value,
    ) -> Result<Value> {
        let command = args["command"].as_str().unwrap_or("");
        let args_vec: Vec<String> = args["args"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        let result =
            crate::core::antibrick::mcp_tools::handle_antibrick_scan(engine, command, args_vec);
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

    pub fn handle_antibrick_enable(
        engine: &AntiBrickEngine,
        id: &Value,
        args: &Value,
    ) -> Result<Value> {
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

    pub fn handle_watchdog_snapshot(
        watchdog: &FilesystemWatchdog,
        id: &Value,
        args: &Value,
    ) -> Result<Value> {
        let path = args["path"].as_str().unwrap_or("/").to_string();
        let result = crate::core::watchdog::mcp_tools::handle_watchdog_snapshot(watchdog, path);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": result.to_string() }] }
        }))
    }

    pub fn handle_watchdog_events(
        watchdog: &FilesystemWatchdog,
        id: &Value,
        args: &Value,
    ) -> Result<Value> {
        let limit = args["limit"].as_u64().unwrap_or(20) as usize;
        let result = crate::core::watchdog::mcp_tools::handle_watchdog_events(watchdog, limit);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": result.to_string() }] }
        }))
    }

    pub fn handle_watchdog_check_path(
        watchdog: &FilesystemWatchdog,
        id: &Value,
        args: &Value,
    ) -> Result<Value> {
        let path = args["path"].as_str().unwrap_or("/").to_string();
        let result = crate::core::watchdog::mcp_tools::handle_watchdog_check_path(watchdog, path);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": result.to_string() }] }
        }))
    }

    pub fn handle_skill_register(
        skills: &SkillRegistry,
        id: &Value,
        args: &Value,
    ) -> Result<Value> {
        let name = args["name"].as_str().unwrap_or("").to_string();
        let description = args["description"].as_str().unwrap_or("").to_string();
        let category_str = args["category"].as_str().unwrap_or("custom");
        if name.is_empty() {
            return Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": "Missing required parameter: name" }
            }));
        }
        let category = category_str
            .parse::<SkillCategory>()
            .unwrap_or(SkillCategory::Custom);
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

    pub fn handle_agent_register(
        agents: &AgentRegistry,
        id: &Value,
        args: &Value,
    ) -> Result<Value> {
        let name = args["name"].as_str().unwrap_or("").to_string();
        let role_str = args["role"].as_str().unwrap_or("general");
        let description = args["description"].as_str().unwrap_or("").to_string();
        if name.is_empty() {
            return Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": "Missing required parameter: name" }
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

    pub fn handle_task_create(
        orchestrator: &Orchestrator,
        id: &Value,
        args: &Value,
    ) -> Result<Value> {
        let title = args["title"].as_str().unwrap_or("Untitled").to_string();
        let description = args["description"].as_str().unwrap_or("").to_string();
        let payload = if description.is_empty() {
            title.clone()
        } else {
            description
        };
        let priority = args["priority"].as_i64().unwrap_or(1) as u8;
        let task_id = orchestrator.create_task(&payload, vec!["developer".into()], priority, None);
        let text = format!("Task created: {} (priority={})", task_id, priority);
        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": text }] }
        }))
    }
    pub fn handle_task_list(orchestrator: &Orchestrator, id: &Value) -> Result<Value> {
        let tasks = orchestrator.list_tasks();
        let text = if tasks.is_empty() {
            "No tasks.".to_string()
        } else {
            let mut lines = vec![format!("Tasks ({}):", tasks.len())];
            for t in &tasks {
                lines.push(format!("- {} [{}]", t.0, t.1));
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
    pub fn handle_mcp_call(id: &Value, args: &Value) -> Result<Value> {
        let server_url = args["server_url"].as_str().unwrap_or("").trim_end_matches('/');
        let tool_name = args["tool_name"].as_str().unwrap_or("");
        let tool_args = args.get("arguments").cloned().unwrap_or(json!({}));
        let endpoint = args["endpoint"].as_str().unwrap_or("/message");

        if server_url.is_empty() || tool_name.is_empty() {
            return Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": "Missing required parameters: server_url and tool_name" }
            }));
        }

        let call_id = MCP_CALL_ID.fetch_add(1, Ordering::SeqCst);
        let request_body = json!({
            "jsonrpc": "2.0", "id": call_id, "method": "tools/call",
            "params": { "name": tool_name, "arguments": tool_args }
        });

        let url = format!("{}{}", server_url, endpoint);
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| e.to_string());

        match client {
            Ok(c) => match c.post(&url).json(&request_body).send() {
                Ok(resp) => {
                    let text = resp.text().unwrap_or_default();
                    Ok(json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": { "content": [{ "type": "text", "text": text }] }
                    }))
                }
                Err(e) => Ok(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "content": [{ "type": "text", "text": format!("MCP call failed: {}", e) }] }
                })),
            },
            Err(e) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "content": [{ "type": "text", "text": format!("HTTP client error: {}", e) }] }
            })),
        }
    }

    pub fn handle_browser_navigate(id: &Value, args: &Value) -> Result<Value> {
        let url = args["url"].as_str().unwrap_or("");
        let method = args["method"].as_str().unwrap_or("GET");
        let extract = args["extract"].as_str().unwrap_or("full");

        if url.is_empty() {
            return Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": "Missing required parameter: url" }
            }));
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| e.to_string());

        let client = match client {
            Ok(c) => c,
            Err(e) => return Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32603, "message": format!("HTTP client error: {}", e) }
            })),
        };

        let response = if method == "POST" {
            let body = args["body"].as_str().unwrap_or("");
            client.post(url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(body.to_string()).send()
        } else {
            client.get(url).send()
        };

        match response {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let content_type = resp.headers().get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown")
                    .to_string();

                let body = resp.text().unwrap_or_default();
                let body_len = body.len();

                let result = match extract {
                    "meta" => {
                        let title = extract_title(&body);
                        let desc = extract_meta(&body, "description");
                        let og_title = extract_meta(&body, "og:title");
                        let og_desc = extract_meta(&body, "og:description");
                        let links = count_links(&body);
                        format!(
                            "URL: {}\nStatus: {}\nTitle: {}\nDescription: {}\nOG Title: {}\nOG Desc: {}\nLinks: {}\nSize: {}",
                            url, status, title, desc, og_title, og_desc, links, format_size2(body_len as u64)
                        )
                    }
                    "links" => {
                        let links = extract_links(&body);
                        let mut out = format!("Links from {} ({}):\n", url, links.len());
                        for (i, link) in links.iter().take(50).enumerate() {
                            out.push_str(&format!("{}. {}\n", i + 1, link));
                        }
                        out
                    }
                    "text" => {
                        let text = strip_html(&body);
                        let preview: String = text.chars().take(3000).collect();
                        format!("Text from {}:\n{}", url, preview)
                    }
                    _ => {
                        let title = extract_title(&body);
                        let text = strip_html(&body);
                        let preview: String = text.chars().take(2000).collect();
                        let links = count_links(&body);
                        format!(
                            "URL: {}\nStatus: {}\nTitle: {}\nType: {}\nSize: {}\nLinks: {}\nContent-Type: {}\n\n--- Page ---\n{}",
                            url, status, title, content_type, format_size2(body_len as u64), links, content_type, preview
                        )
                    }
                };

                Ok(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "content": [{ "type": "text", "text": result }] }
                }))
            }
            Err(e) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "content": [{ "type": "text", "text": format!("Request failed: {}", e) }] }
            })),
        }
    }

    pub fn handle_db_backup(db: &Database, id: &Value, args: &Value) -> Result<Value> {
        let path = args["path"].as_str().unwrap_or("");
        if path.is_empty() {
            return Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": "Missing required parameter: path" }
            }));
        }
        let backup_path = std::path::Path::new(path);
        match db.backup_to(backup_path) {
            Ok(_) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "content": [{ "type": "text", "text": format!("Database backed up to {}", path) }] }
            })),
            Err(e) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32603, "message": format!("Backup failed: {}", e) }
            })),
        }
    }

    pub fn handle_db_integrity(db: &Database, id: &Value) -> Result<Value> {
        match db.integrity_check() {
            Ok(results) => {
                let ok = results.iter().all(|r| r == "ok");
                let text = if ok {
                    "Database integrity: OK".to_string()
                } else {
                    format!("Database integrity issues:\n{}", results.join("\n"))
                };
                Ok(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "content": [{ "type": "text", "text": text }] }
                }))
            }
            Err(e) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32603, "message": format!("Integrity check failed: {}", e) }
            })),
        }
    }

    pub fn handle_db_prune(db: &Database, id: &Value, args: &Value) -> Result<Value> {
        let days = args["older_than_days"].as_i64().unwrap_or(90);
        match db.prune_observations(days) {
            Ok(count) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "content": [{ "type": "text", "text": format!("Pruned {} observations older than {} days", count, days) }] }
            })),
            Err(e) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32603, "message": format!("Prune failed: {}", e) }
            })),
        }
    }

    pub fn handle_db_vacuum(db: &Database, id: &Value) -> Result<Value> {
        match db.vacuum() {
            Ok(_) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "content": [{ "type": "text", "text": "Database vacuum completed." }] }
            })),
            Err(e) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32603, "message": format!("Vacuum failed: {}", e) }
            })),
        }
    }

    pub fn handle_browser_snapshot(id: &Value, args: &Value) -> Result<Value> {
        let url = args["url"].as_str().unwrap_or("");
        let max_text = args["max_text"].as_u64().unwrap_or(2000) as usize;

        if url.is_empty() {
            return Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": "Missing required parameter: url" }
            }));
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| e.to_string());

        let client = match client {
            Ok(c) => c,
            Err(e) => return Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32603, "message": format!("HTTP client error: {}", e) }
            })),
        };

        match client.get(url).send() {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().unwrap_or_default();
                let title = extract_title(&body);
                let desc = extract_meta(&body, "description");
                let text = strip_html(&body);
                let text_preview: String = text.chars().take(max_text).collect();
                let links = count_links(&body);
                let headings = extract_headings(&body);

                let snapshot = format!(
                    "Title: {}\nURL: {}\nStatus: {}\nDescription: {}\nHeadings: {}\nLinks: {}\nText ({}):\n{}",
                    title, url, status, desc, headings, links, text_preview.len(), text_preview
                );

                Ok(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "content": [{ "type": "text", "text": snapshot }] }
                }))
            }
            Err(e) => Ok(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "content": [{ "type": "text", "text": format!("Request failed: {}", e) }] }
            })),
        }
    }
}

fn extract_title(html: &str) -> String {
    if let Some(start) = html.find("<title>") {
        if let Some(end) = html[start + 7..].find("</title>") {
            return html_to_text(&html[start + 7..start + 7 + end]);
        }
    }
    String::new()
}

fn extract_meta(html: &str, name: &str) -> String {
    let patterns = [
        format!("<meta name=\"{}\" content=\"", name),
        format!("<meta property=\"{}\" content=\"", name),
        format!("<meta name='{}' content='", name),
        format!("<meta property='{}' content='", name),
    ];
    for pat in &patterns {
        if let Some(start) = html.find(pat.as_str()) {
            let content_start = start + pat.len();
            if let Some(end) = html[content_start..].find(&['"', '\''][..]) {
                return html_to_text(&html[content_start..content_start + end]);
            }
        }
    }
    String::new()
}

fn extract_links(html: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut pos = 0;
    while let Some(start) = html[pos..].find("<a ") {
        let link_start = pos + start;
        let href = try_extract_href(html, link_start);
        if let Some(href) = href {
            if !href.starts_with('#') && !href.starts_with("javascript:") {
                links.push(href);
            }
            pos = link_start + 3;
        } else {
            pos = link_start + 3;
        }
    }
    links
}

fn try_extract_href(html: &str, start: usize) -> Option<String> {
    let patterns = [("href=\"", '"'), ("href='", '\''), ("href=", ' ')];
    for (prefix, delimiter) in &patterns {
        if let Some(href_start) = html[start..].find(prefix) {
            let val_start = start + href_start + prefix.len();
            if *delimiter == ' ' {
                // unquoted: read until space or >
                let remaining = &html[val_start..];
                let end = remaining.find(|c| c == ' ' || c == '>').unwrap_or(remaining.len());
                return Some(remaining[..end].to_string());
            }
            if let Some(href_end) = html[val_start..].find(*delimiter) {
                return Some(html[val_start..val_start + href_end].to_string());
            }
        }
    }
    None
}

fn count_links(html: &str) -> usize {
    let mut count = 0;
    let mut pos = 0;
    while let Some(start) = html[pos..].find("<a ") {
        count += 1;
        pos += start + 3;
    }
    count
}

fn extract_headings(html: &str) -> String {
    let mut headings = Vec::new();
    for tag in &["h1", "h2", "h3"] {
        let mut pos = 0;
        while let Some(start) = html[pos..].find(&format!("<{}", tag)) {
            let content_start = pos + start;
            if let Some(close) = html[content_start..].find('>') {
                let text_start = content_start + close + 1;
                if let Some(end) = html[text_start..].find(&format!("</{}>", tag)) {
                    let text = html_to_text(&html[text_start..text_start + end]);
                    if !text.is_empty() {
                        headings.push(format!("<{}>{}", tag, text));
                    }
                    pos = text_start + end;
                    continue;
                }
            }
            pos = content_start + 2;
        }
    }
    headings.join("\n")
}

fn strip_html(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut chars = html.chars().peekable();

    while let Some(c) = chars.next() {
        if in_script {
            if c == '<' {
                let mut tag_end = String::new();
                for ch in chars.by_ref() {
                    tag_end.push(ch);
                    if ch == '>' { break; }
                }
                if tag_end.to_lowercase().starts_with("/script>") {
                    in_script = false;
                }
            }
            continue;
        }
        if in_style {
            if c == '<' {
                let mut tag_end = String::new();
                for ch in chars.by_ref() {
                    tag_end.push(ch);
                    if ch == '>' { break; }
                }
                if tag_end.to_lowercase().starts_with("/style>") {
                    in_style = false;
                }
            }
            continue;
        }
        if c == '<' {
            let mut tag_name = String::new();
            let mut rest = String::new();
            for ch in chars.by_ref() {
                if ch == '>' || ch == ' ' {
                    if ch == '>' { break; }
                    rest.push(ch);
                    break;
                }
                tag_name.push(ch);
            }
            let lower = tag_name.to_lowercase();
            if lower == "script" { in_script = true; continue; }
            if lower == "style" { in_style = true; continue; }
            if lower == "br" || lower == "p" || lower == "/p" || lower == "div" || lower == "/div" || lower == "tr" || lower == "/tr" || lower == "li" {
                text.push('\n');
            }
            in_tag = true;
            continue;
        }
        if c == '>' && in_tag {
            in_tag = false;
            continue;
        }
        if !in_tag {
            text.push(c);
        }
    }

    text
}

fn html_to_text(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn format_size2(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    if bytes == 0 { return "0B".into(); }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.1}{}", size, UNITS[unit])
}

/// Derive a persistent encryption key from SYNAPSIS_DB_KEY or a fallback.
/// This ensures encrypted data can always be decrypted within the same session.
fn derive_encryption_key() -> [u8; 32] {
    if let Ok(hex_key) = std::env::var("SYNAPSIS_DB_KEY") {
        if let Ok(decoded) = hex::decode(hex_key) {
            if decoded.len() >= 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&decoded[..32]);
                return key;
            }
        }
    }
    use sha2::Digest;
    let host = hostname::get().map(|h| h.to_string_lossy().to_string()).unwrap_or_default();
    let hash = sha2::Sha256::digest(host.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&hash);
    key
}

fn format_args_snapshot(tool: &str, args: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(obj) = args.as_object() {
        for (key, val) in obj.iter().take(4) {
            let v = match val {
                Value::String(s) => {
                    if s.len() > 80 {
                        let truncated: String = s.chars().take(77).collect();
                        format!("{}...", truncated)
                    } else {
                        s.clone()
                    }
                }
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Array(a) => format!("[{} items]", a.len()),
                Value::Object(o) => format!("{{{}}}", o.keys().take(3).cloned().collect::<Vec<_>>().join(",")),
                _ => "?".to_string(),
            };
            parts.push(format!("{}={}", key, v));
        }
    }
    if parts.len() > 3 {
        parts.truncate(3);
        parts.push("...".into());
    }
    let args_str = if parts.is_empty() { "()".to_string() } else { parts.join(" ") };
    format!("[{}] {}", tool, args_str)
}
