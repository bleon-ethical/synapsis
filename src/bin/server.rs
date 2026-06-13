//! Synapsis Unified MCP Server
//!
//! Single server that supports:
//! - stdio transport (local MCP clients)
//! - HTTP/SSE transport (multi-agent remote)
//!
//! No TCP/raw protocol - only MCP standard.

use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut port: u16 = 7438;
    let mut http_mode = false;

    for (i, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "--http" | "-h" => http_mode = true,
            "--port" | "-p" => {
                if let Some(p) = args.get(i + 1) {
                    port = p.parse().unwrap_or(7438);
                }
            }
            "--help" => {
                println!("Synapsis MCP Server");
                println!("Usage:");
                println!("  synapsis-server              Start MCP server (stdio)");
                println!("  synapsis-server --http       Start MCP server with HTTP/SSE");
                println!("  synapsis-server --http --port PORT  Custom port");
                return;
            }
            _ => {}
        }
    }

    eprintln!("╔══════════════════════════════════════════════════════════╗");
    eprintln!(
        "║  Synapsis v{} - Unified MCP Server               ║",
        env!("CARGO_PKG_VERSION")
    );
    eprintln!("╠══════════════════════════════════════════════════════════╣");

    let db = Arc::new(synapsis::infrastructure::database::Database::new());
    let orchestrator = Arc::new(synapsis::core::orchestrator::Orchestrator::new());
    let server = Arc::new(synapsis::presentation::mcp::McpServer::new(
        db,
        orchestrator,
    ));
    server.init();

    if http_mode {
        eprintln!(
            "║  Transport: HTTP/SSE (port {})                      ║",
            port
        );
        eprintln!("╚══════════════════════════════════════════════════════════╝");
        let transport = synapsis::presentation::http::HttpTransport::new(server);
        transport.start(port);
    } else {
        eprintln!("║  Transport: stdio                                    ║");
        eprintln!("╚══════════════════════════════════════════════════════════╝");
        if let Err(e) = server.run() {
            eprintln!("MCP Server error: {}", e);
            std::process::exit(1);
        }
    }
}
