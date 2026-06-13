//! Synapsis MCP Server - primary interface for AI agent coordination
//!
//! This is the standard MCP implementation that uses stdio for communication.
//! All TCP and Bridge modes have been removed to adhere to the MCP standard.

async fn run_local_mcp() {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("synapsis");
    std::fs::create_dir_all(&data_dir).ok();
    let db_path = data_dir.join("synapsis.db");
    let db_path_str = db_path.to_string_lossy().to_string();
    let db = match synapsis_core::infrastructure::database::Database::new_with_path(&db_path_str) {
        Ok(d) => std::sync::Arc::new(d),
        Err(e) => {
            eprintln!("[MCP] Failed to open persistent database at {}: {}", db_path_str, e);
            eprintln!("[MCP] Falling back to in-memory database");
            std::sync::Arc::new(synapsis_core::infrastructure::database::Database::new())
        }
    };
    let orchestrator = std::sync::Arc::new(synapsis_core::core::orchestrator::Orchestrator::new());
    let server = synapsis::presentation::mcp::McpServer::new(db, orchestrator);
    server.init();

    if let Err(e) = server.run().await {
        eprintln!("MCP Server error: {}", e);
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Synapsis MCP Server");
        println!();
        println!("Usage:");
        println!("  synapsis mcp              Start MCP server (local stdio mode)");
        return;
    }

    eprintln!("╔══════════════════════════════════════════════════════════╗");
    eprintln!(
        "║  Synapsis v{} - MCP Server                           ║",
        env!("CARGO_PKG_VERSION")
    );
    eprintln!("║  MCP Memory Server (Local Mode - Standard)          ║");
    eprintln!("╚══════════════════════════════════════════════════════════╝");
    eprintln!();

    run_local_mcp().await;
}
