use rusqlite::Connection;

use crate::error::Result;

pub fn initialize_database(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON")?;
    conn.execute_batch("PRAGMA journal_mode=WAL")?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS repos (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            remote_url TEXT NOT NULL,
            default_branch TEXT NOT NULL DEFAULT 'main',
            root_path TEXT NOT NULL,
            remote TEXT NOT NULL DEFAULT 'origin',
            display_order INTEGER NOT NULL DEFAULT 0,
            config TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )?;

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_repos_display_order ON repos(display_order)",
    )?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspaces (
            id TEXT PRIMARY KEY,
            repository_id TEXT NOT NULL,
            directory_name TEXT NOT NULL,
            branch TEXT NOT NULL,
            state TEXT NOT NULL DEFAULT 'ready',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(repository_id) REFERENCES repos(id) ON DELETE CASCADE
        )",
    )?;

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_workspaces_repository_id ON workspaces(repository_id)",
    )?;

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_workspaces_state ON workspaces(state)",
    )?;

    // Migrations: add container columns to workspaces
    let _ = conn.execute_batch(
        "ALTER TABLE workspaces ADD COLUMN container_mode TEXT NOT NULL DEFAULT 'local'",
    );
    let _ = conn.execute_batch(
        "ALTER TABLE workspaces ADD COLUMN container_id TEXT",
    );
    // Migrations: track delegation lineage. parent_workspace_id is the
    // workspace that spawned this one (for "what did I delegate?" queries);
    // delegation_prompt is the literal prompt the parent passed.
    let _ = conn.execute_batch(
        "ALTER TABLE workspaces ADD COLUMN parent_workspace_id TEXT",
    );
    let _ = conn.execute_batch(
        "ALTER TABLE workspaces ADD COLUMN delegation_prompt TEXT",
    );

    let _ = conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_workspaces_parent ON workspaces(parent_workspace_id)",
    );

    // Migrations: track the Claude session ID a spawned agent's session
    // settled on (parsed from injected-hook payloads), and the most recent
    // Stop/SubagentStop payload bunyan captured. This replaces the v4
    // <worktree>/result.json writeback so we don't litter the git tree.
    let _ = conn.execute_batch(
        "ALTER TABLE workspaces ADD COLUMN claude_session_id TEXT",
    );
    let _ = conn.execute_batch(
        "ALTER TABLE workspaces ADD COLUMN last_result TEXT",
    );

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_database_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_database(&conn).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"repos".to_string()));
        assert!(tables.contains(&"workspaces".to_string()));
        assert!(tables.contains(&"settings".to_string()));
    }

    #[test]
    fn initialize_database_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_database(&conn).unwrap();
        initialize_database(&conn).unwrap();
    }
}
