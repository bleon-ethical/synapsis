# Contributing to Synapsis

## Standards

### Commit messages — Conventional Commits + scopes

```
<type>(<scope>): <description>

feat(mcp): add db_backup tool
fix(database): escape LIKE wildcards in search_fts
refactor: extract row_to_observation helper
ci: add MSRV check to pipeline
docs: add PR template with database migration checklist
db: add FTS5 migration v2
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `config`, `perf`, `db`, `mcp`, `chore`.
Scopes: `(mcp)`, `(database)`, `(watchdog)`, `(antibrick)`, `(orchestrator)`, `(config)`, `(pqc)`.

### Tool descriptions — must follow format

```json
{
  "name": "<prefix>_<verb>_<noun>",
  "description": "<Verb> <direct object> [optional context].",
  "inputSchema": {
    "type": "object",
    "properties": {
      "param": { "type": "string", "description": "What it does" }
    },
    "required": ["param"]
  }
}
```

- Start with a capital verb: `Save an observation...`, `Search persistent memory...`
- Period at the end.
- All params have `type`, `description`, default values where applicable.

### Output format

```
<Section>
- Key: Value
- List: item
---
<Next section>
```

- No emojis — use `[OK]`, `[FAIL]`, `[*]` markers.
- Errors in `error` field, never `result.content`.
- Success in `result.content` with `type: "text"`.
- No `{:?}` debug format.
- Context tool output: `Context\n- Observations: N\n- Recent chunks: N\n- Project: name`

### Database migrations

1. Add schema change in `create_tables()`.
2. Add migration block:
```rust
if version >= N && version < N+1 {
    conn.execute_batch("...")?;
    conn.execute("INSERT INTO schema_version (version) VALUES (?)", params![N+1])?;
}
```
3. FTS5 index rebuild: add triggers + insert existing data in migration.

### Code style

- No unnecessary `// comments`.
- `info_log!` for info (hidden under `SYNAPSIS_QUIET=1`).
- `debug_log!` for debug (shown with `SYNAPSIS_LOG=debug`).
- `db_warn!` for database warnings (always shown).
- All Mutex/RwLock: `.lock().unwrap_or_else(|e| e.into_inner())`.
- FTS5 preferred for search over `LIKE '%...%'`.
- New tools get entries in: `list_tools()` + `call_tool()` + handler function.

## Pull request lifecycle

```
1. Open PR with conventional commit title
2. Labeler runs → auto-adds type labels
3. CI runs → check → fmt → clippy → test (3 OS) → msrv → security
4. PR Review runs → analyze changes → check conventions → verify DB migrations
5. Auto-approve fires IF:
   - All CI checks pass
   - PR < 500 additions, < 20 files
   - No `breaking` or `blocked` label
   - Database PRs have schema version bump
6. Maintainer merges (squash recommended)
```

## Development

```bash
cargo build --release
cargo test
SYNAPSIS_QUIET=1 cargo run --bin synapsis-mcp

# Test MCP protocol:
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | ./target/release/synapsis-mcp
```

## Release

1. Release drafter maintains changelog from PR labels.
2. Tag `vX.Y.Z` (semver) → CI builds 3 binaries × 5 targets → GitHub Release.
3. Release notes include auto-generated changelog.
