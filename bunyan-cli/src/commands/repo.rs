use clap::Subcommand;

use bunyan_core::models::{CreateRepoInput, Repo, UpdateRepoInput};

use crate::client::BunyanClient;
use crate::output::{self, OutputMode};

#[derive(Subcommand)]
pub enum RepoCommand {
    /// List all repositories
    List,
    /// Get a repository by ID
    Get {
        /// Repository ID
        id: String,
    },
    /// Create a new repository
    Create {
        /// Repository name
        #[arg(long)]
        name: String,
        /// Git remote URL
        #[arg(long)]
        remote_url: String,
        /// Local path for the repo root
        #[arg(long)]
        root_path: String,
        /// Default branch (default: main)
        #[arg(long, default_value = "main")]
        default_branch: String,
        /// Git remote name (default: origin)
        #[arg(long, default_value = "origin")]
        remote: String,
        /// Display order
        #[arg(long, default_value = "0")]
        display_order: i32,
        /// JSON config blob
        #[arg(long)]
        config: Option<String>,
    },
    /// Update a repository
    Update {
        /// Repository ID
        id: String,
        /// New name
        #[arg(long)]
        name: Option<String>,
        /// New default branch
        #[arg(long)]
        default_branch: Option<String>,
        /// New display order
        #[arg(long)]
        display_order: Option<i32>,
        /// JSON config blob
        #[arg(long)]
        config: Option<String>,
    },
    /// Delete a repository
    Delete {
        /// Repository ID
        id: String,
    },
}

pub fn run(client: &BunyanClient, cmd: RepoCommand, mode: OutputMode) {
    match cmd {
        RepoCommand::List => {
            let repos: Vec<Repo> = client.get("/repos").unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
            match mode {
                OutputMode::Quiet => {
                    for r in &repos {
                        println!("{}", r.id);
                    }
                }
                OutputMode::Json => output::print_value(mode, &repos),
                OutputMode::Table => {
                    let rows: Vec<Vec<String>> = repos
                        .iter()
                        .map(|r| vec![r.id.clone(), r.name.clone(), r.default_branch.clone()])
                        .collect();
                    output::print_table(&["ID", "NAME", "BRANCH"], &rows);
                }
            }
        }
        RepoCommand::Get { id } => {
            let repo: Repo = client.get(&format!("/repos/{}", id)).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
            match mode {
                OutputMode::Quiet => println!("{}", repo.id),
                _ => output::print_value(mode, &repo),
            }
        }
        RepoCommand::Create {
            name,
            remote_url,
            root_path,
            default_branch,
            remote,
            display_order,
            config,
        } => {
            let config_val = config.map(|c| {
                serde_json::from_str::<serde_json::Value>(&c).unwrap_or_else(|e| {
                    eprintln!("Invalid JSON config: {}", e);
                    std::process::exit(1);
                })
            });
            let input = CreateRepoInput {
                name,
                remote_url,
                root_path,
                default_branch,
                remote,
                display_order,
                config: config_val,
            };
            let repo: Repo = client.post("/repos", &input).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
            match mode {
                OutputMode::Quiet => println!("{}", repo.id),
                _ => output::print_value(mode, &repo),
            }
        }
        RepoCommand::Update {
            id,
            name,
            default_branch,
            display_order,
            config,
        } => {
            let config_val = config.map(|c| {
                serde_json::from_str::<serde_json::Value>(&c).unwrap_or_else(|e| {
                    eprintln!("Invalid JSON config: {}", e);
                    std::process::exit(1);
                })
            });
            let input = UpdateRepoInput {
                id: id.clone(),
                name,
                default_branch,
                display_order,
                config: config_val,
            };
            let repo: Repo = client
                .put(&format!("/repos/{}", id), &input)
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => println!("{}", repo.id),
                _ => output::print_value(mode, &repo),
            }
        }
        RepoCommand::Delete { id } => {
            let _: () = client
                .delete(&format!("/repos/{}", id))
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            if !matches!(mode, OutputMode::Quiet) {
                println!("Deleted repo {}", id);
            }
        }
    }
}
