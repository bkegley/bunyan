use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::{BunyanError, Result};
use crate::models::{CreateRepoInput, Repo, UpdateRepoInput};

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn row_to_repo(row: &rusqlite::Row) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: row.get(0)?,
        name: row.get(1)?,
        remote_url: row.get(2)?,
        default_branch: row.get(3)?,
        root_path: row.get(4)?,
        remote: row.get(5)?,
        display_order: row.get(6)?,
        conductor_config: row
            .get::<_, Option<String>>(7)?
            .and_then(|s| serde_json::from_str(&s).ok()),
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

const SELECT_COLS: &str =
    "id, name, remote_url, default_branch, root_path, remote, display_order, conductor_config, created_at, updated_at";

pub fn list(conn: &Connection) -> Result<Vec<Repo>> {
    let sql = format!(
        "SELECT {} FROM repos ORDER BY display_order ASC, created_at DESC",
        SELECT_COLS
    );
    let mut stmt = conn.prepare(&sql)?;
    let repos = stmt
        .query_map([], |row| row_to_repo(row))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(repos)
}

pub fn get(conn: &Connection, id: &str) -> Result<Repo> {
    let sql = format!("SELECT {} FROM repos WHERE id = ?1", SELECT_COLS);
    let mut stmt = conn.prepare(&sql)?;
    stmt.query_row([id], |row| row_to_repo(row))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                BunyanError::NotFound(format!("Repository not found: {}", id))
            }
            _ => BunyanError::Database(e),
        })
}

pub fn create(conn: &Connection, input: CreateRepoInput) -> Result<Repo> {
    let id = Uuid::new_v4().to_string();
    let ts = now();
    let config_json = input
        .conductor_config
        .as_ref()
        .map(|v| serde_json::to_string(v))
        .transpose()?;

    conn.execute(
        "INSERT INTO repos (id, name, remote_url, default_branch, root_path, remote, display_order, conductor_config, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            input.name,
            input.remote_url,
            input.default_branch,
            input.root_path,
            input.remote,
            input.display_order,
            config_json,
            ts,
            ts,
        ],
    )?;

    get(conn, &id)
}

pub fn update(conn: &Connection, input: UpdateRepoInput) -> Result<Repo> {
    // Verify it exists first
    let _ = get(conn, &input.id)?;

    let ts = now();
    let mut sets = vec!["updated_at = ?1".to_string()];
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(ts)];
    let mut idx = 2u32;

    if let Some(name) = &input.name {
        sets.push(format!("name = ?{}", idx));
        values.push(Box::new(name.clone()));
        idx += 1;
    }
    if let Some(branch) = &input.default_branch {
        sets.push(format!("default_branch = ?{}", idx));
        values.push(Box::new(branch.clone()));
        idx += 1;
    }
    if let Some(order) = &input.display_order {
        sets.push(format!("display_order = ?{}", idx));
        values.push(Box::new(*order));
        idx += 1;
    }
    if let Some(config) = &input.conductor_config {
        sets.push(format!("conductor_config = ?{}", idx));
        values.push(Box::new(serde_json::to_string(config)?));
        idx += 1;
    }

    let sql = format!(
        "UPDATE repos SET {} WHERE id = ?{}",
        sets.join(", "),
        idx
    );
    values.push(Box::new(input.id.clone()));

    let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, refs.as_slice())?;

    get(conn, &input.id)
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    let affected = conn.execute("DELETE FROM repos WHERE id = ?1", [id])?;
    if affected == 0 {
        return Err(BunyanError::NotFound(format!(
            "Repository not found: {}",
            id
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_database;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        initialize_database(&conn).unwrap();
        conn
    }

    fn sample_input(name: &str) -> CreateRepoInput {
        CreateRepoInput {
            name: name.to_string(),
            remote_url: format!("git@github.com:org/{}.git", name),
            root_path: format!("/repos/{}", name),
            default_branch: "main".to_string(),
            remote: "origin".to_string(),
            display_order: 0,
            conductor_config: None,
        }
    }

    #[test]
    fn create_and_retrieve_repo() {
        let conn = test_db();
        let created = create(&conn, sample_input("frontend")).unwrap();

        assert_eq!(created.name, "frontend");
        assert_eq!(created.remote_url, "git@github.com:org/frontend.git");
        assert_eq!(created.default_branch, "main");

        let fetched = get(&conn, &created.id).unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.name, "frontend");
    }

    #[test]
    fn list_repos_ordered_by_display_order() {
        let conn = test_db();

        let mut input_a = sample_input("alpha");
        input_a.display_order = 2;
        let mut input_b = sample_input("beta");
        input_b.display_order = 1;

        create(&conn, input_a).unwrap();
        create(&conn, input_b).unwrap();

        let repos = list(&conn).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].name, "beta");
        assert_eq!(repos[1].name, "alpha");
    }

    #[test]
    fn update_changes_only_specified_fields() {
        let conn = test_db();
        let created = create(&conn, sample_input("myrepo")).unwrap();

        let updated = update(
            &conn,
            UpdateRepoInput {
                id: created.id.clone(),
                name: Some("renamed".to_string()),
                default_branch: None,
                display_order: Some(5),
                conductor_config: None,
            },
        )
        .unwrap();

        assert_eq!(updated.name, "renamed");
        assert_eq!(updated.display_order, 5);
        assert_eq!(updated.default_branch, "main"); // unchanged
        assert_eq!(updated.remote_url, created.remote_url); // unchanged
    }

    #[test]
    fn delete_makes_repo_unfindable() {
        let conn = test_db();
        let created = create(&conn, sample_input("todelete")).unwrap();

        delete(&conn, &created.id).unwrap();

        let result = get(&conn, &created.id);
        assert!(matches!(result, Err(BunyanError::NotFound(_))));
    }

    #[test]
    fn delete_nonexistent_repo_returns_not_found() {
        let conn = test_db();
        let result = delete(&conn, "nonexistent-id");
        assert!(matches!(result, Err(BunyanError::NotFound(_))));
    }

    #[test]
    fn get_nonexistent_repo_returns_not_found() {
        let conn = test_db();
        let result = get(&conn, "nope");
        assert!(matches!(result, Err(BunyanError::NotFound(_))));
    }

    #[test]
    fn conductor_config_round_trips_as_json() {
        let conn = test_db();
        let config = serde_json::json!({
            "scripts": {
                "setup": "make setup",
                "run": "npm start"
            },
            "runScriptMode": "concurrent"
        });

        let mut input = sample_input("configured");
        input.conductor_config = Some(config.clone());
        let created = create(&conn, input).unwrap();

        assert_eq!(created.conductor_config.unwrap(), config);

        let fetched = get(&conn, &created.id).unwrap();
        assert_eq!(fetched.conductor_config.unwrap(), config);
    }

    #[test]
    fn delete_repo_cascades_to_workspaces() {
        let conn = test_db();
        let repo = create(&conn, sample_input("parent")).unwrap();

        // Create a workspace linked to this repo
        conn.execute(
            "INSERT INTO workspaces (id, repository_id, directory_name, branch, state, created_at, updated_at)
             VALUES ('ws1', ?1, 'lisbon', 'main', 'ready', '2024-01-01', '2024-01-01')",
            [&repo.id],
        )
        .unwrap();

        delete(&conn, &repo.id).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM workspaces WHERE repository_id = ?1",
                [&repo.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
