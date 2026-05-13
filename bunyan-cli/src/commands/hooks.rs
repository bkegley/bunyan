use std::collections::HashMap;

use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::client::BunyanClient;
use crate::output::{self, OutputMode};

#[derive(Subcommand)]
pub enum HooksCommand {
    /// List hook scripts that would run for an event
    List {
        /// Event name (e.g. workspace.ready_to_view)
        event: String,
        /// Repo name to include per-repo hooks
        #[arg(long)]
        repo: Option<String>,
    },
    /// Fire an event against the running daemon (useful for debugging hooks)
    Run {
        /// Event name to fire
        event: String,
        /// Workspace ID to populate context with (optional)
        #[arg(long)]
        workspace: Option<String>,
        /// Extra key=value pairs to pass as `BUNYAN_<KEY>` env vars
        #[arg(long = "extra", value_parser = parse_kv)]
        extras: Vec<(String, String)>,
    },
}

fn parse_kv(s: &str) -> Result<(String, String), String> {
    match s.split_once('=') {
        Some((k, v)) => Ok((k.to_string(), v.to_string())),
        None => Err(format!("expected key=value, got {}", s)),
    }
}

#[derive(Deserialize)]
struct HookListResponse {
    event: String,
    hooks: Vec<String>,
}

#[derive(Serialize)]
struct RunBody {
    event: String,
    workspace_id: Option<String>,
    extras: HashMap<String, String>,
}

#[derive(Deserialize)]
struct HookOutcomeJson {
    path: String,
    exit_code: Option<i32>,
    duration_ms: u128,
    stdout: String,
    stderr: String,
    timed_out: bool,
    succeeded: bool,
}

#[derive(Deserialize)]
struct RunResponse {
    event: String,
    outcomes: Vec<HookOutcomeJson>,
}

pub fn run(client: &BunyanClient, cmd: HooksCommand, mode: OutputMode) {
    match cmd {
        HooksCommand::List { event, repo } => {
            let mut path = format!("/hooks?event={}", urlencode(&event));
            if let Some(r) = repo {
                path.push_str(&format!("&repo={}", urlencode(&r)));
            }
            let resp: HookListResponse = client.get(&path).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
            match mode {
                OutputMode::Quiet => {
                    for h in &resp.hooks {
                        println!("{}", h);
                    }
                }
                OutputMode::Json => output::print_value(
                    mode,
                    &serde_json::json!({"event": resp.event, "hooks": resp.hooks}),
                ),
                OutputMode::Table => {
                    if resp.hooks.is_empty() {
                        println!("(no hooks configured for {})", resp.event);
                    } else {
                        println!("Hooks for {}:", resp.event);
                        for h in &resp.hooks {
                            println!("  {}", h);
                        }
                    }
                }
            }
        }
        HooksCommand::Run {
            event,
            workspace,
            extras,
        } => {
            let extras: HashMap<String, String> = extras.into_iter().collect();
            let body = RunBody {
                event,
                workspace_id: workspace,
                extras,
            };
            let resp: RunResponse = client.post("/hooks/run", &body).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
            match mode {
                OutputMode::Quiet => {
                    for o in &resp.outcomes {
                        println!(
                            "{}\t{}",
                            o.path,
                            o.exit_code
                                .map(|c| c.to_string())
                                .unwrap_or_else(|| "-".into())
                        );
                    }
                }
                OutputMode::Json => output::print_value(
                    mode,
                    &serde_json::json!({
                        "event": resp.event,
                        "outcomes": resp.outcomes.iter().map(|o| serde_json::json!({
                            "path": o.path,
                            "exit_code": o.exit_code,
                            "duration_ms": o.duration_ms,
                            "stdout": o.stdout,
                            "stderr": o.stderr,
                            "timed_out": o.timed_out,
                            "succeeded": o.succeeded,
                        })).collect::<Vec<_>>(),
                    }),
                ),
                OutputMode::Table => {
                    if resp.outcomes.is_empty() {
                        println!("(no hooks ran for {})", resp.event);
                    }
                    for o in &resp.outcomes {
                        let status = if o.timed_out {
                            "TIMEOUT".to_string()
                        } else if o.succeeded {
                            format!("ok ({})", o.exit_code.unwrap_or(0))
                        } else {
                            format!(
                                "fail ({})",
                                o.exit_code
                                    .map(|c| c.to_string())
                                    .unwrap_or_else(|| "-".into())
                            )
                        };
                        println!("{} [{}, {}ms]", o.path, status, o.duration_ms);
                        if !o.stdout.is_empty() {
                            for line in o.stdout.lines() {
                                println!("  out: {}", line);
                            }
                        }
                        if !o.stderr.is_empty() {
                            for line in o.stderr.lines() {
                                println!("  err: {}", line);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn urlencode(s: &str) -> String {
    // Minimal percent-encoder — keep unreserved chars per RFC 3986.
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kv_splits_first_equals() {
        assert_eq!(parse_kv("a=b").unwrap(), ("a".into(), "b".into()));
        assert_eq!(parse_kv("k=v=more").unwrap(), ("k".into(), "v=more".into()));
    }

    #[test]
    fn parse_kv_rejects_missing_equals() {
        assert!(parse_kv("nope").is_err());
    }

    #[test]
    fn urlencode_escapes_dots_in_event_names() {
        assert_eq!(urlencode("workspace.ready_to_view"), "workspace.ready_to_view");
    }

    #[test]
    fn urlencode_escapes_special_chars() {
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("foo/bar"), "foo%2Fbar");
    }
}
