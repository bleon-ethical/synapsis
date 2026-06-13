# Changelog

All notable changes to Synapsis will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-06-12

### Added

- **Persistent database in MCP mode** — `synapsis-mcp` now uses `Database::new_with_path()` to persist observations to `~/.local/share/synapsis/synapsis.db` instead of in-memory. Falls back gracefully to in-memory if disk write fails.
- **Auto DB migration system** — `run_migrations()` tracks schema version via `PRAGMA user_version` and applies incremental migrations automatically. `init()` delegates to the migration pipeline, so databases from v0.4.0 and earlier are transparently upgraded to v1 schema on first open.
- **Synapsis logo** — neural network SVG logo at `assets/logo.svg` displayed in README header.

### Changed

- **Version bump** — v0.4.0 → v0.5.0 (both `synapsis` and `synapsis-core` crates)
- **`mcp.rs`** — `run_local_mcp()` now resolves `dirs::data_local_dir()/synapsis/synapsis.db` and opens it via `new_with_path()`. On failure, logs the error and falls back to `Database::new()` (in-memory).
- **`database.rs`** — `init()` now calls `run_migrations()`. Raw DDL moved into migration v1 block. `schema_version()` helper reads `PRAGMA user_version`.

### Removed

- Old release artifacts under `/home/methodwhite/Proyectos/synapsis/target/release/` (v0.4.0 binary with in-memory-only MCP server).

## [0.4.0] - 2026-06-12

### Added

- **Async MCP server** — fully asynchronous JSON-RPC 2.0 MCP stdio server with proper `initialize`→`initialized`→tool call handshake, supporting all Synapsis MCP tools (`mem_store`, `mem_search`, `mem_recall`, `session_*`, `lock_*`, `task_*`, `db_health`)
- **Semantic search** — embedding-based similarity retrieval across stored memories using importance scoring and budget-based retention policies
- **Chunking pipeline** — intelligent document splitting with configurable chunk size, overlap, and boundary detection strategies for efficient memory ingestion
- **feasibility-analyzer plugin** — built-in plugin exposing 3 MCP tools for project feasibility analysis
- **Dependabot configuration** — automated weekly Cargo dependency updates
- **Clean Architecture modules** — `mcp/`, `cli/`, `tui/`, `db/`, `security/`, `plugins/` layers with clear separation of concerns
- **Multi-platform CI** — build matrix for Linux x86_64, macOS x86_64+aarch64, Windows MSVC

### Changed

- **Massive test expansion** — from ~15 to 50+ tests across all modules (MCP, DB, security, plugins, CLI)
- **Modularization** — monolith split into Clean Architecture with dedicated modules and CI workflows
- **CI/CD overhaul** — multi-platform builds, clippy enforcement, `cargo fmt`, cargo-audit with advisory whitelisting
- **Dependency cleanup** — removed unused `prusia-vault` crate; upgraded `headless_chrome` 0.9→1.0.21 (fixes 0 CVEs)
- **Security fixes** — Kyber handshake hardened, `pqc_encrypt` now returns proper key material, AES KDF strengthened, dead code removed
- **Toolchain pinned** to Rust 1.94.0 for reproducible builds

### Fixed

- `test_throttle_delay` assertion — 0ms delay now accepted as valid on idle
- Zero clippy warnings across all targets (lib, test, bin)
- CI pipeline hidden error echoes removed for clean failure reporting

## [0.3.0] - 2026-05-03

### Fixed
- **Critical**: PersistentEventBus fully implemented with SQLite-backed events table
  - Inter-agent messaging now works (CLI <-> IDE <-> TUI)
  - Direct messages (`send_message`/`get_pending_messages`) between agents
  - Broadcast to channels with polling (`broadcast`/`event_poll`)
  - Event acknowledgment (`event_ack`)
  - Automatic cleanup of expired events
- **Critical**: `McpServer::init()` now calls `db.init()` - tables created on fresh install
- **Critical**: `mem_lock_acquire`/`mem_lock_release` accept both `resource`/`ttl_seconds` (MCP spec) and legacy `lock_key`/`ttl_secs` params
- **Major**: `mw-cli` SynapsisMcpClient rewritten with real JSON-RPC 2.0 MCP stdio communication
  - Full handshake: `initialize` -> `initialized` -> `mem_session_start`
  - All operations use real MCP tool calls instead of mock stubs

### Added
- `events` table in SQLite with indexes for efficient agent/channel/project filtering
- Database methods: `publish_event`, `broadcast_event`, `poll_events`, `get_pending_messages`, `acknowledge_event`, `cleanup_expired_events`
- `connection_status` MCP tool to see active CLI/IDE/TUI connections

## [0.2.0] - 2026-04-02

### Added
- MCP Server Hardening & Installer Infrastructure

## [0.1.0] - 2026-03-22

### Initial Release

- Persistent memory engine with SQLite + FTS5
- MCP server implementation
- TCP server for multi-agent coordination
- PQC security primitives (CRYSTALS-Kyber, CRYSTALS-Dilithium)
- Zero-trust architecture
- Session management with auto-reconnect
- Distributed locks
- Task queue

---

**Security Score:** 8.5/10  
**Last Updated:** 2026-06-12

[Unreleased]: https://github.com/methodwhite/synapsis/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/methodwhite/synapsis/releases/tag/v0.4.0
[0.3.0]: https://github.com/methodwhite/synapsis/releases/tag/v0.3.0
[0.2.0]: https://github.com/methodwhite/synapsis/releases/tag/v0.2.0
[0.1.0]: https://github.com/methodwhite/synapsis/releases/tag/v0.1.0
