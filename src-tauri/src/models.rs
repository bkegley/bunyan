use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub id: String,
    pub name: String,
    pub remote_url: String,
    pub default_branch: String,
    pub root_path: String,
    pub remote: String,
    pub display_order: i64,
    pub conductor_config: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRepoInput {
    pub name: String,
    pub remote_url: String,
    pub root_path: String,
    #[serde(default = "default_branch")]
    pub default_branch: String,
    #[serde(default = "default_remote")]
    pub remote: String,
    #[serde(default)]
    pub display_order: i64,
    pub conductor_config: Option<serde_json::Value>,
}

fn default_branch() -> String {
    "main".to_string()
}
fn default_remote() -> String {
    "origin".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UpdateRepoInput {
    pub id: String,
    pub name: Option<String>,
    pub default_branch: Option<String>,
    pub display_order: Option<i64>,
    pub conductor_config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub repository_id: String,
    pub directory_name: String,
    pub branch: String,
    pub state: WorkspaceState,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceInput {
    pub repository_id: String,
    pub directory_name: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSession {
    pub pid: u32,
    pub workspace_path: String,
    pub workspace_id: Option<String>,
    pub tty: Option<String>,
}
