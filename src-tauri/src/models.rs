use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct Repo {
    pub id: String,
    pub name: String,
    pub remote_url: String,
    pub default_branch: String,
    pub root_path: String,
    pub remote: String,
    pub display_order: i32,
    pub conductor_config: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, specta::Type)]
pub struct CreateRepoInput {
    pub name: String,
    pub remote_url: String,
    pub root_path: String,
    #[serde(default = "default_branch")]
    pub default_branch: String,
    #[serde(default = "default_remote")]
    pub remote: String,
    #[serde(default)]
    pub display_order: i32,
    pub conductor_config: Option<serde_json::Value>,
}

fn default_branch() -> String {
    "main".to_string()
}
fn default_remote() -> String {
    "origin".to_string()
}

#[derive(Debug, Deserialize, specta::Type)]
pub struct UpdateRepoInput {
    pub id: String,
    pub name: Option<String>,
    pub default_branch: Option<String>,
    pub display_order: Option<i32>,
    pub conductor_config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceState {
    Ready,
    Archived,
}

impl WorkspaceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceState::Ready => "ready",
            WorkspaceState::Archived => "archived",
        }
    }

    pub fn from_db(s: &str) -> std::result::Result<Self, String> {
        match s {
            "ready" => Ok(WorkspaceState::Ready),
            "archived" => Ok(WorkspaceState::Archived),
            other => Err(format!("Invalid workspace state: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct Workspace {
    pub id: String,
    pub repository_id: String,
    pub directory_name: String,
    pub branch: String,
    pub state: WorkspaceState,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, specta::Type)]
pub struct CreateWorkspaceInput {
    pub repository_id: String,
    pub directory_name: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ClaudeSession {
    pub pid: u32,
    pub workspace_path: String,
    pub workspace_id: Option<String>,
    pub tty: Option<String>,
}

/// A single session entry from ~/.claude/projects/<path>/sessions-index.json
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ClaudeSessionEntry {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    #[serde(alias = "firstPrompt")]
    pub first_prompt: Option<String>,
    #[serde(alias = "messageCount")]
    pub message_count: Option<i32>,
    pub created: Option<String>,
    pub modified: Option<String>,
    #[serde(alias = "gitBranch")]
    pub git_branch: Option<String>,
    #[serde(alias = "isSidechain")]
    pub is_sidechain: Option<bool>,
}
