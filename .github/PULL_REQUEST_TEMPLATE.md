## Description
<!-- What does this PR do? Why is it needed? -->

## Type of change
- [ ] Bug fix (non-breaking)
- [ ] New feature (non-breaking)
- [ ] Breaking change
- [ ] Database / schema
- [ ] MCP tool
- [ ] Refactor (no functional changes)
- [ ] Documentation

## Checklist
- [ ] `cargo build --release` succeeds
- [ ] `cargo test` passes
- [ ] `cargo clippy` has no new warnings
- [ ] `cargo fmt` has been run
- [ ] No `eprintln!` noise under `SYNAPSIS_QUIET=1`
- [ ] Errors use JSON-RPC `error` field, not `result.content`
- [ ] New tools have `inputSchema` with types, defaults, descriptions
- [ ] Changes are backward-compatible (or marked as breaking)
- [ ] Database migrations have version bump in `schema_version`

## Database changes
- [ ] Schema migration required (version: ___)
- [ ] FTS index needs rebuild
- [ ] No database changes

## Related issues
Closes #...
