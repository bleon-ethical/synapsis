#![recursion_limit = "512"]

//! Synapsis MCP Bridge
//!
//! stdio transport for local MCP clients (Claude Code, Qwen, OpenCode, etc.)
//! Connects to the shared Synapsis state.

fn main() {
    let quiet = std::env::var("SYNAPSIS_QUIET").is_ok() || std::env::var("QUIET").is_ok();

    let db = std::sync::Arc::new(synapsis::infrastructure::database::Database::new());
    let orchestrator = std::sync::Arc::new(synapsis::core::orchestrator::Orchestrator::new());
    let server = synapsis::presentation::mcp::McpServer::new(db, orchestrator);
    server.init();

    if !quiet {
        eprintln!(
            "[synapsis-mcp] v{} ready (stdio)",
            env!("CARGO_PKG_VERSION")
        );
    }

    if let Err(e) = server.run() {
        eprintln!("[synapsis-mcp] Fatal: {}", e);
        std::process::exit(1);
    }
}
