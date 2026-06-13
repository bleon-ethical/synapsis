//! Synapsis MCP Bridge
//!
//! stdio transport for local MCP clients (Claude Code, Qwen, OpenCode, etc.)
//! Connects to the shared Synapsis state.

fn main() {
    let db = std::sync::Arc::new(synapsis::infrastructure::database::Database::new());
    let orchestrator = std::sync::Arc::new(synapsis::core::orchestrator::Orchestrator::new());
    let server = synapsis::presentation::mcp::McpServer::new(db, orchestrator);
    server.init();

    eprintln!("╔══════════════════════════════════════════════════════════╗");
    eprintln!(
        "║  Synapsis v{} - MCP Server (stdio)               ║",
        env!("CARGO_PKG_VERSION")
    );
    eprintln!("║  Transport: stdio                                       ║");
    eprintln!("║  Multi-Agent: shared state via MCP tools                ║");
    eprintln!("╚══════════════════════════════════════════════════════════╝");

    if let Err(e) = server.run() {
        eprintln!("MCP Server error: {}", e);
        std::process::exit(1);
    }
}
