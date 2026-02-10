use clap::Subcommand;

use crate::client::BunyanClient;
use crate::output::{self, OutputMode};

#[derive(Subcommand)]
pub enum PaneCommand {
    /// Kill a pane in a workspace
    Kill {
        /// Workspace ID
        workspace_id: String,
        /// Pane index to kill
        pane_index: u32,
    },
}

pub fn run(client: &BunyanClient, cmd: PaneCommand, mode: OutputMode) {
    match cmd {
        PaneCommand::Kill {
            workspace_id,
            pane_index,
        } => {
            let result: serde_json::Value = client
                .delete(&format!(
                    "/workspaces/{}/panes/{}",
                    workspace_id, pane_index
                ))
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
