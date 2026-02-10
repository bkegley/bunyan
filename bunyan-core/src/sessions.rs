use std::path::Path;

use crate::models::{ClaudeSessionEntry, ContainerMode};

/// Read sessions for a workspace. Tries sessions-index.json first, falls back
/// to scanning JSONL files directly.
pub fn read_sessions(
    workspace_path: &str,
    container_mode: &ContainerMode,
    directory_name: &str,
) -> Result<Vec<ClaudeSessionEntry>, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let sanitized = if *container_mode == ContainerMode::Container {
        format!("/workspace/{}", directory_name).replace('/', "-")
    } else {
        workspace_path.replace('/', "-")
    };
    let project_dir = home.join(".claude").join("projects").join(&sanitized);

    if !project_dir.exists() {
        return Ok(vec![]);
    }

    // Try sessions-index.json first
    let index_path = project_dir.join("sessions-index.json");
    if index_path.exists() {
        if let Ok(sessions) = read_sessions_from_index(&index_path) {
            return Ok(sessions);
        }
    }

    // Fall back to scanning JSONL files
    read_sessions_from_jsonl(&project_dir)
}

fn read_sessions_from_index(index_path: &Path) -> Result<Vec<ClaudeSessionEntry>, String> {
    let content = std::fs::read_to_string(index_path)
        .map_err(|e| format!("Failed to read sessions-index.json: {}", e))?;

    #[derive(serde::Deserialize)]
    struct SessionsIndex {
        entries: Vec<ClaudeSessionEntry>,
    }

    let index: SessionsIndex = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse sessions-index.json: {}", e))?;

    let mut sessions: Vec<ClaudeSessionEntry> = index
        .entries
        .into_iter()
        .filter(|e| !e.is_sidechain.unwrap_or(false))
        .collect();
    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));

    Ok(sessions)
}

/// Scan .jsonl files in a project directory and extract session metadata.
fn read_sessions_from_jsonl(project_dir: &Path) -> Result<Vec<ClaudeSessionEntry>, String> {
    let entries = std::fs::read_dir(project_dir)
        .map_err(|e| format!("Failed to read project directory: {}", e))?;

    let mut sessions = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }

        let session_id = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let modified = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| {
                let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                let dt = chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)?;
                Some(dt.to_rfc3339())
            });

        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;

        let mut first_prompt = None;
        let mut created = None;
        let mut git_branch = None;
        let mut is_sidechain = None;
        let mut message_count: i32 = 0;

        for line in reader.lines().take(50) {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let val: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let msg_type = val.get("type").and_then(|t| t.as_str());

            if msg_type == Some("user") || msg_type == Some("assistant") {
                message_count += 1;
            }

            if msg_type == Some("user") && first_prompt.is_none() {
                first_prompt = val
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string());
                created = val
                    .get("timestamp")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
                git_branch = val
                    .get("gitBranch")
                    .and_then(|b| b.as_str())
                    .map(|s| s.to_string());
                is_sidechain = val.get("isSidechain").and_then(|b| b.as_bool());
            }
        }

        if is_sidechain == Some(true) {
            continue;
        }

        sessions.push(ClaudeSessionEntry {
            session_id,
            first_prompt,
            message_count: Some(message_count),
            created,
            modified,
            git_branch,
            is_sidechain,
        });
    }

    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(sessions)
}

/// Check if a workspace has any existing Claude sessions.
pub fn has_existing_session(
    workspace_path: &str,
    container_mode: &ContainerMode,
    directory_name: &str,
) -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };

    let sanitized = if *container_mode == ContainerMode::Container {
        format!("/workspace/{}", directory_name).replace('/', "-")
    } else {
        workspace_path.replace('/', "-")
    };
    let sessions_path = home
        .join(".claude")
        .join("projects")
        .join(&sanitized)
        .join("sessions-index.json");

    sessions_path.exists()
}
