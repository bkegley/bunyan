use clap::Subcommand;

use bunyan_core::models::{ClaudeSessionEntry, ContainerMode, CreateWorkspaceInput, TmuxPane, Workspace};

use crate::client::BunyanClient;
use crate::output::{self, OutputMode};

#[derive(Subcommand)]
pub enum WorkspaceCommand {
    /// List workspaces (optionally filter by repo)
    List {
        /// Filter by repository ID
        #[arg(long)]
        repo_id: Option<String>,
    },
    /// Get a workspace by ID
    Get {
        /// Workspace ID
        id: String,
    },
    /// Create a new workspace (worktree)
    Create {
        /// Repository ID
        #[arg(long)]
        repo: String,
        /// Directory name for the worktree
        #[arg(long)]
        name: String,
        /// Git branch name
        #[arg(long)]
        branch: String,
        /// Use container mode
        #[arg(long)]
        container: bool,
    },
    /// Archive a workspace
    Archive {
        /// Workspace ID
        id: String,
    },
    /// View workspace in iTerm
    View {
        /// Workspace ID
        id: String,
    },
    /// List session history for a workspace
    Sessions {
        /// Workspace ID
        id: String,
    },
    /// List tmux panes for a workspace
    Panes {
        /// Workspace ID
        id: String,
    },
}

pub fn run(client: &BunyanClient, cmd: WorkspaceCommand, mode: OutputMode) {
    match cmd {
        WorkspaceCommand::List { repo_id } => {
            let path = match &repo_id {
                Some(id) => format!("/workspaces?repo_id={}", id),
                None => "/workspaces".to_string(),
            };
            let workspaces: Vec<Workspace> = client.get(&path).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
            match mode {
                OutputMode::Quiet => {
                    for w in &workspaces {
                        println!("{}", w.id);
                    }
                }
                OutputMode::Json => output::print_value(mode, &workspaces),
                OutputMode::Table => {
                    let rows: Vec<Vec<String>> = workspaces
                        .iter()
                        .map(|w| {
                            vec![
                                w.id.clone(),
                                w.directory_name.clone(),
                                w.branch.clone(),
                                w.state.as_str().to_string(),
                                w.container_mode.as_str().to_string(),
                            ]
                        })
                        .collect();
                    output::print_table(&["ID", "NAME", "BRANCH", "STATE", "MODE"], &rows);
                }
            }
        }
        WorkspaceCommand::Get { id } => {
            let ws: Workspace = client
                .get(&format!("/workspaces/{}", id))
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => println!("{}", ws.id),
                _ => output::print_value(mode, &ws),
            }
        }
        WorkspaceCommand::Create {
            repo,
            name,
            branch,
            container,
        } => {
            let input = CreateWorkspaceInput {
                repository_id: repo,
                directory_name: name,
                branch,
                container_mode: if container {
                    ContainerMode::Container
                } else {
                    ContainerMode::Local
                },
            };
            let ws: Workspace = client.post("/workspaces", &input).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
            match mode {
                OutputMode::Quiet => println!("{}", ws.id),
                _ => output::print_value(mode, &ws),
            }
        }
        WorkspaceCommand::Archive { id } => {
            let ws: Workspace = client
                .post_empty(&format!("/workspaces/{}/archive", id))
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => println!("{}", ws.id),
                _ => output::print_value(mode, &ws),
            }
        }
        WorkspaceCommand::View { id } => {
            let result: serde_json::Value = client
                .post_empty(&format!("/workspaces/{}/view", id))
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => {}
                _ => output::print_value(mode, &result),
            }
        }
        WorkspaceCommand::Sessions { id } => {
            let sessions: Vec<ClaudeSessionEntry> = client
                .get(&format!("/workspaces/{}/sessions", id))
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => {
                    for s in &sessions {
                        println!("{}", s.session_id);
                    }
                }
                OutputMode::Json => output::print_value(mode, &sessions),
                OutputMode::Table => {
                    let rows: Vec<Vec<String>> = sessions
                        .iter()
                        .map(|s| {
                            vec![
                                s.session_id.clone(),
                                s.first_prompt.clone().unwrap_or_default(),
                                s.message_count
                                    .map(|c| c.to_string())
                                    .unwrap_or_default(),
                                s.modified.clone().unwrap_or_default(),
                            ]
                        })
                        .collect();
                    output::print_table(&["SESSION_ID", "PROMPT", "MESSAGES", "MODIFIED"], &rows);
                }
            }
        }
        WorkspaceCommand::Panes { id } => {
            let panes: Vec<TmuxPane> = client
                .get(&format!("/workspaces/{}/panes", id))
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => {
                    for p in &panes {
                        println!("{}", p.pane_index);
                    }
                }
                OutputMode::Json => output::print_value(mode, &panes),
                OutputMode::Table => {
                    let rows: Vec<Vec<String>> = panes
                        .iter()
                        .map(|p| {
                            vec![
                                p.pane_index.to_string(),
                                p.command.clone(),
                                if p.is_active {
                                    "*".to_string()
                                } else {
                                    "".to_string()
                                },
                            ]
                        })
                        .collect();
                    output::print_table(&["INDEX", "COMMAND", "ACTIVE"], &rows);
                }
            }
        }
    }
}
