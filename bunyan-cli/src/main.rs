mod client;
mod commands;
mod config;
mod output;

use clap::{Parser, Subcommand};

use client::BunyanClient;
use output::OutputMode;

#[derive(Parser)]
#[command(name = "bunyan", about = "CLI for the Bunyan workspace manager")]
struct Cli {
    /// Server port override
    #[arg(long, global = true)]
    port: Option<u16>,

    /// Output raw JSON
    #[arg(long, global = true)]
    json: bool,

    /// Output only IDs (quiet mode)
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Repository management
    Repo {
        #[command(subcommand)]
        cmd: commands::repo::RepoCommand,
    },
    /// Workspace management
    #[command(alias = "ws")]
    Workspace {
        #[command(subcommand)]
        cmd: commands::workspace::WorkspaceCommand,
    },
    /// Claude session management
    Session {
        #[command(subcommand)]
        cmd: commands::session::SessionCommand,
    },
    /// Pane management
    Pane {
        #[command(subcommand)]
        cmd: commands::pane::PaneCommand,
    },
    /// Docker operations
    Docker {
        #[command(subcommand)]
        cmd: commands::docker::DockerCommand,
    },
    /// Settings management
    Settings {
        #[command(subcommand)]
        cmd: commands::settings::SettingsCommand,
    },
    /// Check server health and Docker availability
    Status,
    /// Start the headless bunyan server
    Serve {
        /// Port to listen on (default: 3333)
        #[arg(long, default_value = "3333")]
        port: u16,
    },
}

fn main() {
    let cli = Cli::parse();

    let mode = if cli.quiet {
        OutputMode::Quiet
    } else if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Table
    };

    match cli.command {
        Command::Serve { port } => {
            std::env::set_var("BUNYAN_PORT", port.to_string());
            let state = bunyan_core::init_state();
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(bunyan_core::server::start_server(state, port));
        }
        cmd => {
            let base_url = config::discover_server_url(cli.port);
            let client = BunyanClient::new(&base_url);

            match cmd {
                Command::Repo { cmd: sub } => commands::repo::run(&client, sub, mode),
                Command::Workspace { cmd: sub } => commands::workspace::run(&client, sub, mode),
                Command::Session { cmd: sub } => commands::session::run(&client, sub, mode),
                Command::Pane { cmd: sub } => commands::pane::run(&client, sub, mode),
                Command::Docker { cmd: sub } => commands::docker::run(&client, sub, mode),
                Command::Settings { cmd: sub } => commands::settings::run(&client, sub, mode),
                Command::Status => run_status(&client, mode),
                Command::Serve { .. } => unreachable!(),
            }
        }
    }
}

fn run_status(client: &BunyanClient, mode: OutputMode) {
    let health: Result<serde_json::Value, String> = client.get("/health");
    let docker: Result<serde_json::Value, String> = client.get("/docker/status");

    let server_ok = health.is_ok();
    let docker_available = docker
        .as_ref()
        .ok()
        .and_then(|v| v.get("available"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    match mode {
        OutputMode::Quiet => {
            if server_ok {
                println!("ok");
            } else {
                println!("unreachable");
                std::process::exit(1);
            }
        }
        OutputMode::Json => {
            let status = serde_json::json!({
                "server": if server_ok { "ok" } else { "unreachable" },
                "docker": if docker_available { "available" } else { "unavailable" },
            });
            output::print_value(mode, &status);
            if !server_ok {
                std::process::exit(1);
            }
        }
        OutputMode::Table => {
            println!(
                "Server: {}",
                if server_ok { "ok" } else { "unreachable" }
            );
            println!(
                "Docker: {}",
                if docker_available {
                    "available"
                } else {
                    "unavailable"
                }
            );
            if !server_ok {
                std::process::exit(1);
            }
        }
    }
}
