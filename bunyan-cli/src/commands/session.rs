use clap::Subcommand;
use serde::Serialize;

use bunyan_core::models::WorkspacePaneInfo;

use crate::client::BunyanClient;
use crate::output::{self, OutputMode};

#[derive(Subcommand)]
pub enum SessionCommand {
    /// List all active Claude sessions across workspaces
    Active,
    /// Open a new Claude session in a workspace
    Open {
        /// Workspace ID
        workspace_id: String,
    },
    /// Resume an existing Claude session
    Resume {
        /// Workspace ID
        workspace_id: String,
        /// Session ID to resume
        session_id: String,
    },
    /// Open a shell pane in a workspace
    Shell {
        /// Workspace ID
        workspace_id: String,
    },
}

#[derive(Serialize)]
struct ResumeBody {
    session_id: String,
}

pub fn run(client: &BunyanClient, cmd: SessionCommand, mode: OutputMode) {
    match cmd {
        SessionCommand::Active => {
            let sessions: Vec<WorkspacePaneInfo> =
                client.get("/sessions/active").unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => {
                    for s in &sessions {
                        println!("{}", s.workspace_id);
                    }
                }
                OutputMode::Json => output::print_value(mode, &sessions),
                OutputMode::Table => {
                    let rows: Vec<Vec<String>> = sessions
                        .iter()
                        .map(|s| {
                            vec![
                                s.workspace_id.clone(),
                                s.repo_name.clone(),
                                s.workspace_name.clone(),
                                s.panes.len().to_string(),
                            ]
                        })
                        .collect();
                    output::print_table(&["WORKSPACE_ID", "REPO", "WORKSPACE", "PANES"], &rows);
                }
            }
        }
        SessionCommand::Open { workspace_id } => {
            let result: serde_json::Value = client
                .post_empty(&format!("/workspaces/{}/claude", workspace_id))
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => {}
                _ => output::print_value(mode, &result),
            }
        }
        SessionCommand::Resume {
            workspace_id,
            session_id,
        } => {
            let body = ResumeBody { session_id };
            let result: serde_json::Value = client
                .post(
                    &format!("/workspaces/{}/claude/resume", workspace_id),
                    &body,
                )
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => {}
                _ => output::print_value(mode, &result),
            }
        }
        SessionCommand::Shell { workspace_id } => {
            let result: serde_json::Value = client
                .post_empty(&format!("/workspaces/{}/shell", workspace_id))
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => {}
                _ => output::print_value(mode, &result),
            }
        }
    }
}
