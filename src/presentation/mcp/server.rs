//! Synapsis MCP Server Implementation
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use synapsis_core::core::antibrick::{AntiBrickConfig, AntiBrickEngine};
use synapsis_core::core::orchestrator::Orchestrator;
use synapsis_core::core::watchdog::FilesystemWatchdog;
use synapsis_core::core::PqcryptoProvider;
use synapsis_core::domain::crypto::{CryptoProvider, PqcAlgorithm};
use synapsis_core::domain::*;
use synapsis_core::infrastructure::agents::AgentRegistry;
use synapsis_core::infrastructure::database::Database;
use synapsis_core::infrastructure::plugin::PluginManager;
use synapsis_core::infrastructure::skills::SkillRegistry;

use crate::app_core::resources::ResourceMonitor;
use crate::infrastructure::cache::memory_cache::MemoryCache;
use crate::plugins::security_shield;

use super::resources;

#[derive(Debug, Clone)]
pub(crate) struct ConnectionInfo {
    pub(crate) client_name: String,
    pub(crate) client_type: String,
    pub(crate) connected_at: Instant,
    pub(crate) last_activity: Instant,
    pub(crate) protocol: String,
    pub(crate) status: ConnectionStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConnectionStatus {
    Connected,
    Idle,
    Disconnected,
}

pub(crate) struct PersistentEventBus {
    db: Arc<Database>,
}

pub(crate) struct PublishParams<'a> {
    pub(crate) event_type: &'a str,
    pub(crate) from: &'a str,
    pub(crate) to: Option<&'a str>,
    pub(crate) project: Option<&'a str>,
    pub(crate) channel: &'a str,
    pub(crate) content: &'a str,
    pub(crate) priority: i32,
}

impl PersistentEventBus {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub(crate) fn publish(&self, params: PublishParams) -> Result<i64, String> {
        self.db
            .publish_event(
                params.event_type,
                params.from,
                params.to,
                params.project,
                params.channel,
                params.content,
                params.priority,
            )
            .map_err(|e| e.to_string())
    }

    pub(crate) fn broadcast(
        &self,
        event_type: &str,
        from: &str,
        project: Option<&str>,
        channel: &str,
        content: &str,
        priority: i32,
    ) -> Result<i64, String> {
        self.db
            .broadcast_event(event_type, from, project, channel, content, priority)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn poll(
        &self,
        since: i64,
        channel: Option<&str>,
        project: Option<&str>,
        limit: i32,
    ) -> Result<Vec<serde_json::Value>, String> {
        self.db
            .poll_events(since, channel, project, limit)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn get_pending_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<serde_json::Value>, String> {
        self.db
            .get_pending_messages(session_id)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn mark_read(&self, event_id: i64) -> Result<(), String> {
        self.db
            .acknowledge_event(event_id)
            .map_err(|e| e.to_string())
    }
}

pub(crate) struct QueryCache {
    pub(crate) search: MemoryCache<Vec<synapsis_core::domain::entities::SearchResult>>,
    pub(crate) chunks: MemoryCache<Vec<serde_json::Value>>,
    pub(crate) context: MemoryCache<serde_json::Value>,
    pub(crate) stats: MemoryCache<serde_json::Value>,
}

impl QueryCache {
    pub(crate) fn with_ttl(ttl: Duration) -> Self {
        Self {
            search: MemoryCache::new(Some(ttl)).with_max_entries(256),
            chunks: MemoryCache::new(Some(ttl)).with_max_entries(128),
            context: MemoryCache::new(Some(ttl)).with_max_entries(128),
            stats: MemoryCache::new(Some(Duration::from_secs(5))).with_max_entries(32),
        }
    }

    pub(crate) fn invalidate_all(&self) {
        self.search.clear();
        self.chunks.clear();
        self.context.clear();
        self.stats.clear();
    }

    pub(crate) fn invalidate_search(&self) {
        self.search.clear();
    }
}

pub struct McpServer {
    pub(crate) db: Arc<Database>,
    pub(crate) skills: Arc<SkillRegistry>,
    pub(crate) agents: Arc<AgentRegistry>,
    pub(crate) orchestrator: Arc<Orchestrator>,
    pub(crate) antibrick: Arc<AntiBrickEngine>,
    pub(crate) watchdog: Arc<FilesystemWatchdog>,
    pub(crate) client_name: Arc<RwLock<Option<String>>>,
    pub(crate) persistent_event_bus: Arc<PersistentEventBus>,
    pub(crate) plugin_manager: Arc<PluginManager>,
    pub(crate) crypto_provider: Arc<dyn CryptoProvider>,
    pub(crate) resource_monitor: Arc<ResourceMonitor>,
    pub(crate) cache: QueryCache,
    pub(crate) connections: Arc<Mutex<HashMap<String, ConnectionInfo>>>,
    pub(crate) shutdown_requested: Arc<AtomicBool>,
    pub(crate) updater: Arc<crate::app_core::updater::AutoUpdater>,
}

impl McpServer {
    pub fn new(db: Arc<Database>, orchestrator: Arc<Orchestrator>) -> Self {
        let plugin_dir = dirs::data_local_dir()
            .map(|mut d| {
                d.push("synapsis");
                d.push("plugins");
                d
            })
            .unwrap_or_else(|| PathBuf::from("./synapsis_plugins"));

        let persistent_event_bus = Arc::new(PersistentEventBus::new(db.clone()));

        Self {
            db: db.clone(),
            skills: Arc::new(SkillRegistry::new()),
            agents: Arc::new(AgentRegistry::new()),
            orchestrator,
            antibrick: Arc::new(AntiBrickEngine::new(AntiBrickConfig::default())),
            watchdog: Arc::new(FilesystemWatchdog::new(Default::default())),
            client_name: Arc::new(RwLock::new(None)),
            persistent_event_bus,
            plugin_manager: Arc::new(PluginManager::with_path(plugin_dir)),
            crypto_provider: Arc::new(PqcryptoProvider::new()),
            resource_monitor: Arc::new(ResourceMonitor::new()),
            cache: QueryCache::with_ttl(Duration::from_secs(30)),
            connections: Arc::new(Mutex::new(HashMap::new())),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
            updater: Arc::new(crate::app_core::updater::AutoUpdater::new(db.clone())),
        }
    }

    pub fn init(&self) {
        if let Err(e) = self.db.init() {
            eprintln!("[MCP] Warning: Database init failed: {}", e);
        }

        self.skills.init().ok();
        self.agents.init().ok();
        self.watchdog.start_monitoring();

        security_shield::init_default_rules();

        eprintln!("[MCP] Rust Server Initialized (watchdog + security shield started)");
    }

    pub(crate) fn get_agent_id(&self) -> String {
        let client_name_lock = self.client_name.read().unwrap();
        client_name_lock
            .as_deref()
            .unwrap_or("mcp-session")
            .to_string()
    }

    pub(crate) fn get_session_id(&self) -> types::SessionId {
        let client_name_lock = self.client_name.read().unwrap();
        let cli_type = client_name_lock.as_deref().unwrap_or("mcp-session");
        types::SessionId::new(cli_type)
    }

    pub async fn run(&self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = tokio::io::BufReader::new(stdin);

        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).await? == 0 {
                break;
            }

            if let Some(resp_str) = self.handle_message(&line) {
                stdout.write_all(resp_str.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }

            if self.shutdown_requested.load(Ordering::SeqCst) {
                break;
            }
        }

        Ok(())
    }

    pub fn handle_message(&self, message: &str) -> Option<String> {
        let request: Value = match serde_json::from_str(message) {
            Ok(v) => v,
            Err(_) => {
                return Some(
                    json!({
                        "jsonrpc": "2.0",
                        "error": { "code": -32700, "message": "Invalid JSON" }
                    })
                    .to_string(),
                )
            }
        };
        let request_id = request["id"].clone();
        let is_notification = request_id.is_null();
        match self.handle_request(request) {
            Ok(response) => {
                if is_notification {
                    None
                } else {
                    serde_json::to_string(&response).ok()
                }
            }
            Err(e) => {
                if is_notification {
                    None
                } else {
                    let err_resp = json!({
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "error": { "code": -32603, "message": e.to_string() }
                    });
                    serde_json::to_string(&err_resp).ok()
                }
            }
        }
    }

    fn extract_args<'a>(&self, params: &'a Value) -> &'a Value {
        if let Some(args) = params.get("arguments") {
            args
        } else {
            params
        }
    }

    fn handle_request(&self, request: Value) -> Result<Value> {
        let method = request["method"].as_str().unwrap_or("");
        let id = &request["id"];

        if let Some(client_name) = self.client_name.read().unwrap().as_ref() {
            let mut connections = self.connections.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(conn) = connections.get_mut(client_name) {
                conn.last_activity = Instant::now();
            }
        }

        match method {
            "initialize" => {
                let client_protocol = request["params"]["protocolVersion"]
                    .as_str()
                    .unwrap_or("2024-11-05");
                let client_name = request["params"]["clientInfo"]["name"]
                    .as_str()
                    .unwrap_or("mcp-client")
                    .to_string();
                {
                    let mut client_name_lock =
                        self.client_name.write().unwrap_or_else(|e| e.into_inner());
                    *client_name_lock = Some(client_name.clone());
                }
                let connection_id = client_name.clone();
                let mut connections = self.connections.lock().unwrap_or_else(|e| e.into_inner());
                connections.insert(
                    connection_id,
                    ConnectionInfo {
                        client_name: client_name.clone(),
                        client_type: "unknown".to_string(),
                        connected_at: Instant::now(),
                        last_activity: Instant::now(),
                        protocol: "mcp-stdin".to_string(),
                        status: ConnectionStatus::Connected,
                    },
                );
                Ok(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": client_protocol,
                        "capabilities": {
                            "tools": { "listChanged": true },
                            "resources": { "listChanged": true },
                            "prompts": { "listChanged": true }
                        },
                        "serverInfo": {
                            "name": "synapsis",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                }))
            }
            "initialized" => Ok(json!(null)),
            "shutdown" => {
                self.shutdown_requested.store(true, Ordering::SeqCst);
                Ok(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": null
                }))
            }
            "$/cancelRequest" => Ok(json!(null)),
            "tools/list" => self.list_tools(id),
            "tools/call" => self.call_tool(id, &request["params"]),
            "resources/list" => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "resources": super::resources::list_resources() }
            })),
            "prompts/list" => Ok(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "prompts": [{ "name": "memory_context" }] }
            })),
            "prompts/get" => {
                let name = request["params"]["name"].as_str().unwrap_or("");
                if name == "memory_context" {
                    let args = &request["params"]["arguments"];
                    match self.action_mem_context(args) {
                        Ok(ctx) => Ok(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": { "messages": [{ "role": "user", "content": { "type": "text", "text": serde_json::to_string(&ctx).unwrap() } }] }
                        })),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                } else {
                    Ok(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": "Prompt not found" }
                    }))
                }
            }
            _ => {
                let args = self.extract_args(&request["params"]);
                let tool_params = json!({ "name": method, "arguments": args });
                self.call_tool(id, &tool_params)
            }
        }
    }
}
