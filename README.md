# Synapsis - Persistent Memory Engine for AI Agents (Rust)

[![Release](https://img.shields.io/github/v/tag/MethodWhite/synapsis?label=release)](https://github.com/MethodWhite/synapsis/releases)
[![Rust](https://img.shields.io/badge/rust-v1.88-orange.svg)](https://www.rust-lang.org)
[![PQC](https://img.shields.io/badge/PQC-Kyber%20%2B%20Dilithium-blue)](docs/KYBER_REAL_PROOF.md)
[![License](https://img.shields.io/badge/license-BUSL--1.1-red.svg)](LICENSE)
[![Last Commit](https://img.shields.io/github/last-commit/MethodWhite/synapsis)](https://github.com/MethodWhite/synapsis/commits/main)

**Synapsis** is a pure Rust persistent memory engine for AI agents with post-quantum cryptography (PQC), multi-agent orchestration, and native MCP protocol integration. Zero Python dependencies.

> `/ˈsɪnæpsɪs/` — the structure that enables neurons to communicate.

---

## Quick Start

### Automatic Install

```bash
curl -fsSL https://raw.githubusercontent.com/methodwhite/synapsis/main/install.sh | bash
```

Or build from source (requires Rust 1.88+):

```bash
git clone https://github.com/methodwhite/synapsis.git
cd synapsis
cargo build --release
```

### Update

```bash
synapsis update    # Auto-detects latest GitHub release and updates
synapsis --version # Check current version
```

### Start MCP Server

```bash
synapsis mcp    # JSON-RPC over stdio for IDE/CLI/TUI integration
```

---

## Architecture

```
CLI (opencode, qwen, mw-cli)  IDE (vscode, cursor, jetbrains)  TUI (mw-cli)
         │                              │                              │
         └──────────────────────────────┼──────────────────────────────┘
                                        │
                              MCP JSON-RPC (stdio)
                                        │
                    ┌───────────────────▼───────────────────┐
                    │           SYNAPSIS MCP               │
                    │  Memory │ Events │ Agents │ Tasks    │
                    └───────────────────┬───────────────────┘
                                        │
                          SQLite + FTS5 + SQLCipher
                          events table (inter-agent bus)
```

---

## Key Features

### Inter-Agent Communication (v0.3.0)
- **Persistent Event Bus**: SQLite-backed `events` table for real-time messaging between agents
- Direct messages (`send_message`/`get_pending_messages`) between CLI, IDE, and TUI agents
- Channel broadcasts with polling (`broadcast`/`event_poll`)
- Event acknowledgment and automatic cleanup of expired events

### Security (10/10 Verified)
- CRYSTALS-Kyber-512 key encapsulation
- CRYSTALS-Dilithium-2 digital signatures
- AES-256-GCM encryption with SQLCipher at rest
- HMAC-SHA256 session integrity
- Zero-trust architecture with continuous verification

### Performance
| Metric | Synapsis (Rust) | Traditional (Go) |
|--------|-----------------|------------------|
| Binary Size | <5 MB | ~15 MB |
| Memory RSS | <20 MB | ~50 MB |
| Search Latency | <1 ms | ~5 ms |
| Cold Start | <20 ms | ~100 ms |

---

## Supported Platforms

| Platform | Install | Notes |
|----------|---------|-------|
| **Linux** (x86_64, aarch64) | `install.sh` | Native |
| **macOS** (Intel, Apple Silicon) | `install-macos.sh` | Native |
| **Windows** (WSL2, PowerShell) | `install.ps1` | Native |
| **Android** (Termux) | `install.sh` | ARM64 |
| **iPhoneOS** (via iSH/a-Shell) | `cargo build --target aarch64-apple-ios` | Cross-compile |

---

## MCP Tools (50+)

### Memory
| Tool | Description |
|------|-------------|
| `mem_save` | Save observation with PQC integrity hash |
| `mem_search` | FTS5 search with BM25 ranking |
| `mem_context` | Relevant context chunks |
| `mem_timeline` | Chronological history |
| `mem_update` | Update with audit trail |
| `mem_delete` | Soft-delete with recovery |

### Events & Messaging
| Tool | Description |
|------|-------------|
| `send_message` | Direct message to another agent |
| `get_pending_messages` | Retrieve messages for agent |
| `broadcast` | Broadcast to channel |
| `event_poll` | Poll events since timestamp |

### Agents & Tasks
| Tool | Description |
|------|-------------|
| `agent_heartbeat` | Health monitoring |
| `agent_details` | Agent status |
| `task_create` | Create task |
| `task_claim` | Claim from queue |
| `task_complete` | Mark complete |
| `mem_lock_acquire` | Distributed lock |
| `mem_lock_release` | Release lock |

### Security
| Tool | Description |
|------|-------------|
| `security_classify` | Risk analysis |
| `security_sanitize_input` | Injection prevention |
| `pqc_encrypt` | Post-quantum encryption |
| `cve_search` | NVD database search |
| `security_audit` | Full security audit |

---

## Supported MCP Clients

| Client | Status | Protocol |
|--------|--------|----------|
| OpenCode | Active | stdio |
| Qwen Code | Active | stdio |
| mw-cli (TUI) | Active | stdio |
| Claude Code | Supported | stdio |
| Cursor | Supported | stdio |
| VS Code | Supported | stdio |
| Windsurf | Supported | stdio |
| JetBrains | Supported | stdio |
| Gemini CLI | Supported | stdio |
| aichat | Supported | stdio |

---

## Agent Coordination

```bash
# All agents share the same Synapsis database
# Automatic session management with auto-reconnect
# Distributed locking for resource coordination
# Task queue for multi-agent workflows
# Real-time inter-agent messaging via events table
```

### Resource Management

| Feature | Description |
|---------|-------------|
| System Monitoring | Real-time CPU, memory, load tracking |
| Adaptive Throttling | Automatic delay based on load |
| Agent Limits | Per-agent concurrency caps |
| Priority Scheduling | Critical tasks get resources first |

---

## Project Structure

```
synapsis/
├── src/
│   ├── main.rs           # Binary entry (synapsis)
│   ├── bin/mcp.rs        # MCP server binary (synapsis-mcp)
│   ├── lib.rs            # Library root
│   ├── app_core/         # App-specific core logic
│   ├── cli/              # CLI parser
│   ├── presentation/     # MCP, HTTP servers
│   ├── tools/            # Tool implementations
│   └── plugins/          # Plugin bridge modules
├── docs/                 # Security, architecture docs
├── install.sh            # Linux/Android installer
├── install.ps1           # Windows installer
├── install-macos.sh      # macOS installer
├── Cargo.toml
└── CHANGELOG.md
```

---

## Dependencies

Pure Rust ecosystem. Zero Python. Key dependencies:

| Crate | Purpose |
|-------|---------|
| `rusqlite` (SQLCipher) | Encrypted storage |
| `serde` / `serde_json` | Serialization |
| `tokio` | Async runtime |
| `pqcrypto-kyber` / `pqcrypto-dilithium` | Post-quantum crypto |
| `prusia-vault` | Secure key management |
| `self_update` | Automatic updates via GitHub releases |

---

## Testing

```bash
cargo test                # All tests
cargo test --features security  # Security tests
cargo build --release     # Release build
```

---

## Version History

| Version | Date | Highlights |
|---------|------|------------|
| **v0.3.0** | 2026-05-03 | Inter-agent event bus, db.init() fix, mw-cli MCP client, pure Rust |
| v0.2.0 | 2026-04-02 | MCP hardening, installer infrastructure |
| v0.1.0 | 2026-03-22 | Initial release |

See [CHANGELOG.md](CHANGELOG.md) for details.

---

## License

**BUSL-1.1** (Business Source License 1.1). Personal, educational, and research use. Commercial use requires license.

Contact: methodwhite@proton.me

---

**Built with Rust by MethodWhite**
