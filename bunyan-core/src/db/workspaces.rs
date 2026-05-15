use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::{BunyanError, Result};
use crate::models::{ContainerMode, CreateWorkspaceInput, Workspace, WorkspaceState};

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn row_to_workspace(row: &rusqlite::Row) -> rusqlite::Result<Workspace> {
    let state_str: String = row.get(4)?;
    let container_mode_str: String = row.get(5)?;
    Ok(Workspace {
        id: row.get(0)?,
        repository_id: row.get(1)?,
        directory_name: row.get(2)?,
        branch: row.get(3)?,
        state: WorkspaceState::from_db(&state_str)
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
        container_mode: ContainerMode::from_db(&container_mode_str)
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
        container_id: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        parent_workspace_id: row.get(9)?,
        delegation_prompt: row.get(10)?,
        claude_session_id: row.get(11)?,
        last_result: row.get(12)?,
    })
}

const SELECT_COLS: &str =
    "id, repository_id, directory_name, branch, state, container_mode, container_id, created_at, updated_at, parent_workspace_id, delegation_prompt, claude_session_id, last_result";

pub fn list(conn: &Connection, repository_id: Option<&str>) -> Result<Vec<Workspace>> {
    list_filtered(conn, &ListFilters {
        repository_id: repository_id.map(|s| s.to_string()),
        ..Default::default()
    })
}

#[derive(Debug, Default, Clone)]
pub struct ListFilters {
    pub repository_id: Option<String>,
    pub state: Option<WorkspaceState>,
    pub parent_workspace_id: Option<String>,
    /// ISO-8601 timestamp; rows with `created_at >= since` only.
    pub since: Option<String>,
}

pub fn list_filtered(conn: &Connection, filters: &ListFilters) -> Result<Vec<Workspace>> {
    let mut sql = format!("SELECT {} FROM workspaces WHERE 1=1", SELECT_COLS);
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(repo) = &filters.repository_id {
        sql.push_str(" AND repository_id = ?");
        params.push(Box::new(repo.clone()));
    }
    if let Some(state) = &filters.state {
        sql.push_str(" AND state = ?");
        params.push(Box::new(state.as_str().to_string()));
    }
    if let Some(parent) = &filters.parent_workspace_id {
        sql.push_str(" AND parent_workspace_id = ?");
        params.push(Box::new(parent.clone()));
    }
    if let Some(since) = &filters.since {
        sql.push_str(" AND created_at >= ?");
        params.push(Box::new(since.clone()));
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| row_to_workspace(row))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> Result<Workspace> {
    let sql = format!("SELECT {} FROM workspaces WHERE id = ?1", SELECT_COLS);
    let mut stmt = conn.prepare(&sql)?;
    stmt.query_row([id], |row| row_to_workspace(row))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                BunyanError::NotFound(format!("Workspace not found: {}", id))
            }
            _ => BunyanError::Database(e),
        })
}

pub fn create(conn: &Connection, input: CreateWorkspaceInput) -> Result<Workspace> {
    create_with_lineage(conn, input, None, None)
}

/// Like `create`, but also records the parent workspace and delegation prompt.
/// Used by `POST /delegate` so observation endpoints can answer
/// "what did workspace X spawn?" and "what was the spawn prompt?"
pub fn create_with_lineage(
    conn: &Connection,
    input: CreateWorkspaceInput,
    parent_workspace_id: Option<&str>,
    delegation_prompt: Option<&str>,
) -> Result<Workspace> {
    // Verify the repo exists
    crate::db::repos::get(conn, &input.repository_id)?;

    let id = Uuid::new_v4().to_string();
    let ts = now();

    conn.execute(
        "INSERT INTO workspaces (id, repository_id, directory_name, branch, state, container_mode, created_at, updated_at, parent_workspace_id, delegation_prompt)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            input.repository_id,
            input.directory_name,
            input.branch,
            WorkspaceState::Ready.as_str(),
            input.container_mode.as_str(),
            ts,
            ts,
            parent_workspace_id,
            delegation_prompt,
        ],
    )?;

    get(conn, &id)
}

pub fn archive(conn: &Connection, id: &str) -> Result<Workspace> {
    let ts = now();
    let affected = conn.execute(
        "UPDATE workspaces SET state = ?1, updated_at = ?2 WHERE id = ?3 AND state = ?4",
        params![
            WorkspaceState::Archived.as_str(),
            ts,
            id,
            WorkspaceState::Ready.as_str(),
        ],
    )?;

    if affected == 0 {
        // Check if it exists at all vs already archived
        let ws = get(conn, id)?;
        if ws.state == WorkspaceState::Archived {
            return Ok(ws);
        }
        return Err(BunyanError::NotFound(format!(
            "Workspace not found: {}",
            id
        )));
    }

    get(conn, id)
}

pub fn set_container_id(conn: &Connection, id: &str, container_id: &str) -> Result<()> {
    let ts = now();
    conn.execute(
        "UPDATE workspaces SET container_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![container_id, ts, id],
    )?;
    Ok(())
}

pub fn count_container_workspaces(conn: &Connection, repo_id: &str) -> Result<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM workspaces WHERE repository_id = ?1 AND container_mode = 'container' AND state = 'ready'",
        params![repo_id],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Record the Claude session ID a spawned agent settled on. Idempotent
/// — overwrites whatever was there (Claude can resume into the same
/// workspace with a new session, and the latest one wins).
pub fn set_claude_session_id(conn: &Connection, id: &str, session_id: &str) -> Result<()> {
    let ts = now();
    conn.execute(
        "UPDATE workspaces SET claude_session_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![session_id, ts, id],
    )?;
    Ok(())
}

/// Store the most recent Stop/SubagentStop payload as a JSON string.
/// Replaces the v4 on-disk result.json writeback.
pub fn set_last_result(conn: &Connection, id: &str, payload_json: &str) -> Result<()> {
    let ts = now();
    conn.execute(
        "UPDATE workspaces SET last_result = ?1, updated_at = ?2 WHERE id = ?3",
        params![payload_json, ts, id],
    )?;
    Ok(())
}

pub fn clear_container_id(conn: &Connection, id: &str) -> Result<()> {
    let ts = now();
    let null: Option<&str> = None;
    conn.execute(
        "UPDATE workspaces SET container_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![null, ts, id],
    )?;
    Ok(())
}

#[allow(dead_code)]
pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    let affected = conn.execute("DELETE FROM workspaces WHERE id = ?1", [id])?;
    if affected == 0 {
        return Err(BunyanError::NotFound(format!(
            "Workspace not found: {}",
            id
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repos;
    use crate::db::schema::initialize_database;
    use crate::models::{ContainerMode, CreateRepoInput};

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        initialize_database(&conn).unwrap();
        conn
    }

    fn create_test_repo(conn: &Connection, name: &str) -> crate::models::Repo {
        repos::create(
            conn,
            CreateRepoInput {
                name: name.to_string(),
                remote_url: format!("git@github.com:org/{}.git", name),
                root_path: format!("/repos/{}", name),
                default_branch: "main".to_string(),
                remote: "origin".to_string(),
                display_order: 0,
                config: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn create_workspace_links_to_repo() {
        let conn = test_db();
        let repo = create_test_repo(&conn, "frontend");

        let ws = create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id.clone(),
                directory_name: "lisbon".to_string(),
                branch: "bkegley/lisbon".to_string(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();

        assert_eq!(ws.repository_id, repo.id);
        assert_eq!(ws.directory_name, "lisbon");
        assert_eq!(ws.branch, "bkegley/lisbon");
    }

    #[test]
    fn create_workspace_for_nonexistent_repo_fails() {
        let conn = test_db();
        let result = create(
            &conn,
            CreateWorkspaceInput {
                repository_id: "nonexistent".to_string(),
                directory_name: "lisbon".to_string(),
                branch: "main".to_string(),
                container_mode: ContainerMode::Local,
            },
        );
        assert!(matches!(result, Err(BunyanError::NotFound(_))));
    }

    #[test]
    fn new_workspaces_start_in_ready_state() {
        let conn = test_db();
        let repo = create_test_repo(&conn, "frontend");

        let ws = create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id,
                directory_name: "chicago".to_string(),
                branch: "main".to_string(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();

        assert_eq!(ws.state, WorkspaceState::Ready);
    }

    #[test]
    fn archive_workspace_changes_state() {
        let conn = test_db();
        let repo = create_test_repo(&conn, "frontend");

        let ws = create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id,
                directory_name: "boston".to_string(),
                branch: "main".to_string(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();

        let archived = archive(&conn, &ws.id).unwrap();
        assert_eq!(archived.state, WorkspaceState::Archived);

        // Fetching again confirms persistence
        let fetched = get(&conn, &ws.id).unwrap();
        assert_eq!(fetched.state, WorkspaceState::Archived);
    }

    #[test]
    fn list_with_repo_filter_returns_only_that_repos_workspaces() {
        let conn = test_db();
        let repo1 = create_test_repo(&conn, "repo1");
        let repo2 = create_test_repo(&conn, "repo2");

        create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo1.id.clone(),
                directory_name: "ws1".to_string(),
                branch: "main".to_string(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();

        create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo2.id.clone(),
                directory_name: "ws2".to_string(),
                branch: "main".to_string(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();

        let repo1_ws = list(&conn, Some(&repo1.id)).unwrap();
        assert_eq!(repo1_ws.len(), 1);
        assert_eq!(repo1_ws[0].directory_name, "ws1");

        let repo2_ws = list(&conn, Some(&repo2.id)).unwrap();
        assert_eq!(repo2_ws.len(), 1);
        assert_eq!(repo2_ws[0].directory_name, "ws2");
    }

    #[test]
    fn list_without_filter_returns_all() {
        let conn = test_db();
        let repo1 = create_test_repo(&conn, "repo1");
        let repo2 = create_test_repo(&conn, "repo2");

        create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo1.id,
                directory_name: "ws1".to_string(),
                branch: "main".to_string(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();

        create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo2.id,
                directory_name: "ws2".to_string(),
                branch: "main".to_string(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();

        let all = list(&conn, None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn list_filtered_by_state_returns_only_matching_state() {
        let conn = test_db();
        let repo = create_test_repo(&conn, "frontend");
        let ready = create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id.clone(),
                directory_name: "ready-one".into(),
                branch: "r1".into(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();
        let to_archive = create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id,
                directory_name: "to-archive".into(),
                branch: "a1".into(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();
        archive(&conn, &to_archive.id).unwrap();

        let only_ready = list_filtered(
            &conn,
            &ListFilters {
                state: Some(WorkspaceState::Ready),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(only_ready.len(), 1);
        assert_eq!(only_ready[0].id, ready.id);

        let only_archived = list_filtered(
            &conn,
            &ListFilters {
                state: Some(WorkspaceState::Archived),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(only_archived.len(), 1);
        assert_eq!(only_archived[0].id, to_archive.id);
    }

    #[test]
    fn list_filtered_by_delegated_by_returns_children_of_parent() {
        let conn = test_db();
        let repo = create_test_repo(&conn, "frontend");
        let parent = create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id.clone(),
                directory_name: "parent".into(),
                branch: "p".into(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();
        let child = create_with_lineage(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id.clone(),
                directory_name: "child".into(),
                branch: "c".into(),
                container_mode: ContainerMode::Local,
            },
            Some(&parent.id),
            Some("fix the flaky test"),
        )
        .unwrap();
        // An unrelated peer
        create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id,
                directory_name: "peer".into(),
                branch: "x".into(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();

        let kids = list_filtered(
            &conn,
            &ListFilters {
                parent_workspace_id: Some(parent.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].id, child.id);
        assert_eq!(kids[0].parent_workspace_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(kids[0].delegation_prompt.as_deref(), Some("fix the flaky test"));
    }

    #[test]
    fn create_with_lineage_records_parent_and_prompt() {
        let conn = test_db();
        let repo = create_test_repo(&conn, "frontend");
        let ws = create_with_lineage(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id,
                directory_name: "side".into(),
                branch: "side".into(),
                container_mode: ContainerMode::Local,
            },
            Some("parent-id"),
            Some("do the thing"),
        )
        .unwrap();
        assert_eq!(ws.parent_workspace_id.as_deref(), Some("parent-id"));
        assert_eq!(ws.delegation_prompt.as_deref(), Some("do the thing"));
    }

    #[test]
    fn archive_already_archived_workspace_is_idempotent() {
        let conn = test_db();
        let repo = create_test_repo(&conn, "frontend");

        let ws = create(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id,
                directory_name: "denver".to_string(),
                branch: "main".to_string(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap();

        archive(&conn, &ws.id).unwrap();
        let second = archive(&conn, &ws.id).unwrap();
        assert_eq!(second.state, WorkspaceState::Archived);
    }
}
