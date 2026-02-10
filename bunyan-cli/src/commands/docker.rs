use clap::Subcommand;

use bunyan_core::models::PortMapping;

use crate::client::BunyanClient;
use crate::output::{self, OutputMode};

#[derive(Subcommand)]
pub enum DockerCommand {
    /// Check if Docker daemon is available
    Status,
    /// Get container status for a workspace
    ContainerStatus {
        /// Workspace ID
        workspace_id: String,
    },
    /// Get port mappings for a workspace container
    Ports {
        /// Workspace ID
        workspace_id: String,
    },
}

pub fn run(client: &BunyanClient, cmd: DockerCommand, mode: OutputMode) {
    match cmd {
        DockerCommand::Status => {
            let result: serde_json::Value =
                client.get("/docker/status").unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => {
                    let available = result
                        .get("available")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if available {
                        println!("available");
                    } else {
                        println!("unavailable");
                    }
                }
                _ => output::print_value(mode, &result),
            }
        }
        DockerCommand::ContainerStatus { workspace_id } => {
            let result: serde_json::Value = client
                .get(&format!(
                    "/workspaces/{}/container/status",
                    workspace_id
                ))
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => {
                    let status = result
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    println!("{}", status);
                }
                _ => output::print_value(mode, &result),
            }
        }
        DockerCommand::Ports { workspace_id } => {
            let ports: Vec<PortMapping> = client
                .get(&format!("/workspaces/{}/container/ports", workspace_id))
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => {
                    for p in &ports {
                        println!("{}:{}", p.container_port, p.host_port);
                    }
                }
                OutputMode::Json => output::print_value(mode, &ports),
                OutputMode::Table => {
                    let rows: Vec<Vec<String>> = ports
                        .iter()
                        .map(|p| {
                            vec![
                                p.container_port.clone(),
                                p.host_port.clone(),
                                p.host_ip.clone(),
                            ]
                        })
                        .collect();
                    output::print_table(&["CONTAINER_PORT", "HOST_PORT", "HOST_IP"], &rows);
                }
            }
        }
    }
}
