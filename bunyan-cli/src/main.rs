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
    /// Start the bunyan server in the foreground
    Serve {
        /// Port to listen on (default: 3333)
        #[arg(long, default_value = "3333")]
        port: u16,
    },
    /// Start the bunyan server as a background daemon
    Up {
        /// Port to listen on (default: 3333)
        #[arg(long, default_value = "3333")]
        port: u16,
    },
    /// Stop the running bunyan daemon
    Down,
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
        Command::Up { port } => {
            run_up(port);
        }
        Command::Down => {
            run_down(cli.port);
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
                Command::Serve { .. } | Command::Up { .. } | Command::Down => unreachable!(),
            }
        }
    }
}

fn run_up(port: u16) {
    let url = format!("http://127.0.0.1:{}/health", port);

    // Check if already running
    if ureq::get(&url).call().map(|r| r.status() == 200).unwrap_or(false) {
        eprintln!("bunyan daemon already running on port {}", port);
        return;
    }

    // Spawn ourselves with `serve` in the background
    let self_bin = std::env::current_exe().expect("Cannot determine own binary path");

    std::process::Command::new(&self_bin)
        .args(["serve", "--port", &port.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("Failed to spawn bunyan serve");

    // Wait for ready
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if ureq::get(&url).call().map(|r| r.status() == 200).unwrap_or(false) {
            eprintln!("bunyan daemon started on port {}", port);
            return;
        }
    }
    eprintln!("bunyan daemon did not start within 5 seconds");
    std::process::exit(1);
}

fn run_down(port_override: Option<u16>) {
    let base_url = config::discover_server_url(port_override);
    let url = format!("{}/health", base_url);

    if !ureq::get(&url).call().map(|r| r.status() == 200).unwrap_or(false) {
        eprintln!("bunyan daemon is not running");
        return;
    }

    // Read PID from port file and send SIGTERM
    let port_file = dirs::home_dir()
        .expect("Cannot determine home directory")
        .join(".bunyan")
        .join("server.port");

    // The server listens for SIGTERM and cleans up, but we need its PID.
    // Simplest: find the process listening on the port.
    let port = port_override
        .or_else(|| std::env::var("BUNYAN_PORT").ok().and_then(|p| p.parse().ok()))
        .or_else(|| {
            std::fs::read_to_string(&port_file)
                .ok()
                .and_then(|s| s.trim().parse().ok())
        })
        .unwrap_or(3333);

    let output = std::process::Command::new("lsof")
        .args(["-ti", &format!(":{}", port)])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let pids = String::from_utf8_lossy(&out.stdout);
            for pid in pids.trim().lines() {
                if let Ok(pid_num) = pid.trim().parse::<i32>() {
                    unsafe {
                        libc::kill(pid_num, libc::SIGTERM);
                    }
                }
            }
            eprintln!("bunyan daemon stopped");
        }
        _ => {
            eprintln!("Could not find bunyan daemon process");
            std::process::exit(1);
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
