use anyhow::Result;

pub const MIGRATIONS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY);",
    "INSERT OR IGNORE INTO schema_version (version) VALUES (1);",
];

const MIGRATION_SCRIPTS: &[&str] = &[
    // v1: Initial schema
    "CREATE TABLE IF NOT EXISTS memories (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        content TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );",
    "CREATE INDEX IF NOT EXISTS idx_memories_session_id ON memories(session_id);",
    "CREATE TABLE IF NOT EXISTS agents (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        last_heartbeat TEXT,
        metadata TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );",
    "CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY,
        description TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',
        priority TEXT NOT NULL DEFAULT 'normal',
        assignee TEXT,
        result TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );",
];

pub fn run_migrations(db: &rusqlite::Connection) -> Result<()> {
    let current_version: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for (i, script) in MIGRATION_SCRIPTS.iter().enumerate() {
        let version = (i + 1) as i64;
        if version > current_version {
            db.execute_batch(script)?;
            db.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                rusqlite::params![version],
            )?;
        }
    }

    Ok(())
}
