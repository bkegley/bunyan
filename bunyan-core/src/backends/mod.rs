//! Runtime backends: pluggable strategies for keeping workspace processes alive.
//!
//! A backend is "the thing that owns the Claude session and shell panes for a
//! workspace." Today that's tmux; tomorrow it might be zellij or a native
//! process supervisor. The `RuntimeBackend` trait is the LCD across these,
//! deliberately small.
//!
//! Routes and other callers must go through the trait — direct imports of
//! `tmux::*` are no longer allowed.

pub mod tmux;
pub mod zellij;

use std::sync::Arc;

use crate::error::Result;
use crate::models::TmuxPane;

/// A handle to a process that a backend is keeping alive. Backends define
/// the format (e.g. `"tmux:bunyan/frontend:fix.0"`); to the rest of bunyan
/// this is opaque.
pub type ProcessHandle = String;

/// Snapshot of a process the backend is supervising.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Backend-opaque handle.
    pub handle: ProcessHandle,
    /// Current command running (e.g. "claude", "zsh").
    pub command: String,
    /// Whether this slot is currently the focused/active one.
    pub is_active: bool,
    /// Working directory of the process.
    pub cwd: String,
    /// OS PID of the process root.
    pub pid: u32,
    /// Numeric slot index *within the workspace* (e.g. tmux pane index).
    /// Used by routes that address processes by index.
    pub slot_index: u32,
}

impl ProcessInfo {
    /// Convenience: turn a backend `ProcessInfo` into the wire-level `TmuxPane`
    /// model. We keep the wire name `TmuxPane` until a v6/v7 rename can hit
    /// API consumers — but the model is now backend-agnostic.
    pub fn to_pane(&self) -> TmuxPane {
        TmuxPane {
            pane_index: self.slot_index,
            command: self.command.clone(),
            is_active: self.is_active,
            workspace_path: self.cwd.clone(),
            pane_pid: self.pid,
        }
    }
}

/// All bunyan operations that touch the multiplexer go through this trait.
///
/// **Implementation contract:**
/// - All methods are synchronous and should be called from a blocking task.
/// - "Workspace" is a (repo, name) pair; backends pick how to model that
///   internally (tmux uses session+window, zellij would use session+tab).
/// - Methods that "ensure" something are idempotent.
/// - Methods that don't find their target should return an `Ok(empty)` /
///   `Ok(None)` rather than an error, except where the proposal calls out
///   a hard failure.
pub trait RuntimeBackend: Send + Sync {
    /// Human-readable backend name, e.g. "tmux", "zellij", "native".
    fn name(&self) -> &'static str;

    /// Ensure the workspace's container (tmux window, zellij tab, ...) exists
    /// and is rooted at `workspace_path`. Idempotent.
    fn ensure_workspace(
        &self,
        repo_name: &str,
        workspace_name: &str,
        workspace_path: &str,
    ) -> Result<()>;

    /// Tear down the workspace's container. Best-effort: a missing target
    /// is not an error.
    fn kill_workspace(&self, repo_name: &str, workspace_name: &str) -> Result<()>;

    /// List processes the backend is supervising for this workspace.
    /// Returns `Ok(vec![])` if the workspace doesn't exist.
    fn list_processes(
        &self,
        repo_name: &str,
        workspace_name: &str,
    ) -> Result<Vec<ProcessInfo>>;

    /// List every process the backend is supervising, grouped by workspace.
    /// Returns `(repo_name, workspace_name, process)` triples.
    fn list_all_processes(&self) -> Result<Vec<(String, String, ProcessInfo)>>;

    /// Spawn a new process slot running `cmd` in the workspace, ensuring the
    /// workspace exists first.
    fn spawn(
        &self,
        repo_name: &str,
        workspace_name: &str,
        workspace_path: &str,
        cmd: &str,
    ) -> Result<()>;

    /// Send `cmd` into an existing process slot (e.g. shell pane).
    fn send_to_slot(
        &self,
        repo_name: &str,
        workspace_name: &str,
        slot_index: u32,
        cmd: &str,
    ) -> Result<()>;

    /// Kill a single process slot by its numeric index within the workspace.
    fn kill_slot(&self, repo_name: &str, workspace_name: &str, slot_index: u32) -> Result<()>;

    /// Return a shell command a UI can run to attach a TTY to this workspace.
    /// For tmux this is `tmux -L bunyan attach -t <repo>`; for native it
    /// might be `bunyan process attach <id>`.
    fn attach_command(&self, repo_name: &str) -> String;

    /// Backend-specific way to identify a process slot that's currently
    /// running a claude session with the given ID. Backends that can't tell
    /// the difference can return `Ok(None)`.
    ///
    /// Default implementation walks `list_processes` and asks
    /// `extract_session_id_for_process` per slot — backends with a more
    /// efficient lookup may override.
    fn find_slot_running_session(
        &self,
        repo_name: &str,
        workspace_name: &str,
        session_id: &str,
    ) -> Result<Option<u32>> {
        let processes = self.list_processes(repo_name, workspace_name)?;
        for p in processes {
            if is_shell(&p.command) {
                continue;
            }
            if let Some(running) = self.extract_session_id_for_process(&p) {
                if running == session_id {
                    return Ok(Some(p.slot_index));
                }
            }
        }
        Ok(None)
    }

    /// Inspect a process and return the claude session ID it's running, if
    /// any. Default implementation peeks at the process tree.
    fn extract_session_id_for_process(&self, info: &ProcessInfo) -> Option<String> {
        extract_session_id_from_pid_tree(info.pid)
    }

    /// Find the index of the first idle process slot (one running just a
    /// shell). Default walks `list_processes`.
    fn find_idle_slot(&self, repo_name: &str, workspace_name: &str) -> Result<Option<u32>> {
        let processes = self.list_processes(repo_name, workspace_name)?;
        for p in processes {
            if is_shell(&p.command) {
                return Ok(Some(p.slot_index));
            }
        }
        Ok(None)
    }

    /// Whether any process slot in the workspace is running something
    /// other than a known shell (i.e. claude). Default walks `list_processes`.
    fn has_claude_running(&self, repo_name: &str, workspace_name: &str) -> Result<bool> {
        let processes = self.list_processes(repo_name, workspace_name)?;
        Ok(processes.iter().any(|p| !is_shell(&p.command)))
    }
}

/// Construct the default backend (tmux for now).
pub fn default_backend() -> Arc<dyn RuntimeBackend> {
    Arc::new(tmux::TmuxBackend::new())
}

/// Construct a backend by its name string. Returns `None` for unknown names
/// so the caller can fall back to the default.
pub fn backend_by_name(name: &str) -> Option<Arc<dyn RuntimeBackend>> {
    match name {
        "tmux" => Some(Arc::new(tmux::TmuxBackend::new())),
        "zellij" => Some(Arc::new(zellij::ZellijBackend::new())),
        _ => None,
    }
}

/// Resolve which backend to use based on bunyan's user config and the repo
/// the caller is asking about. Repo-level config overrides the top-level
/// `[runtime].backend` setting.
///
/// `config_dir` is typically `~/.config/bunyan`; `repo_name` is the
/// optional repo we're resolving for.
pub fn resolve_backend(
    config_dir: Option<&std::path::Path>,
    repo_name: Option<&str>,
) -> Arc<dyn RuntimeBackend> {
    let name = config_dir
        .and_then(|dir| read_backend_from_config(dir, repo_name))
        .unwrap_or_else(|| "tmux".to_string());
    backend_by_name(&name).unwrap_or_else(default_backend)
}

fn read_backend_from_config(
    config_dir: &std::path::Path,
    repo_name: Option<&str>,
) -> Option<String> {
    let config_path = config_dir.join("config.toml");
    let body = std::fs::read_to_string(&config_path).ok()?;
    parse_backend_choice(&body, repo_name)
}

/// Parse a minimal subset of bunyan's config.toml — enough to find the
/// `[runtime] backend = "..."` line and the per-repo override
/// `[runtime.repos.<name>] backend = "..."`. Avoids pulling in a full TOML
/// dependency; bunyan-core's only TOML use is this lookup.
fn parse_backend_choice(toml_body: &str, repo_name: Option<&str>) -> Option<String> {
    let mut top_level: Option<String> = None;
    let mut current_section = String::new();
    let mut by_repo: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for raw_line in toml_body.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current_section = rest.trim().to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        // Strip inline `# ...` comments, then surrounding quotes.
        let value = value
            .split('#')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"')
            .to_string();
        if key != "backend" {
            continue;
        }
        match current_section.as_str() {
            "runtime" => top_level = Some(value),
            section if section.starts_with("runtime.repos.") => {
                let name = section.trim_start_matches("runtime.repos.").to_string();
                by_repo.insert(name, value);
            }
            _ => {}
        }
    }

    if let Some(name) = repo_name {
        if let Some(per_repo) = by_repo.get(name) {
            return Some(per_repo.clone());
        }
    }
    top_level
}

const SHELLS: &[&str] = &["zsh", "bash", "fish", "sh"];

pub fn is_shell(cmd: &str) -> bool {
    SHELLS.contains(&cmd)
}

fn extract_session_id_from_pid_tree(pane_pid: u32) -> Option<String> {
    let pid_str = pane_pid.to_string();
    if let Some(id) = extract_session_id_from_pid(&pid_str) {
        return Some(id);
    }
    let output = std::process::Command::new("pgrep")
        .args(["-P", &pid_str])
        .output()
        .ok()?;
    for child_pid in std::str::from_utf8(&output.stdout).ok()?.lines() {
        if let Some(id) = extract_session_id_from_pid(child_pid) {
            return Some(id);
        }
    }
    None
}

fn extract_session_id_from_pid(pid: &str) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-p", pid, "-o", "args="])
        .output()
        .ok()?;
    let args = std::str::from_utf8(&output.stdout).ok()?.trim().to_string();
    if let Some(id) = args.strip_prefix("claude --resume ") {
        return Some(id.trim().to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_backend_choice_returns_top_level() {
        let body = r#"
            [runtime]
            backend = "zellij"
        "#;
        assert_eq!(
            parse_backend_choice(body, None),
            Some("zellij".to_string())
        );
    }

    #[test]
    fn parse_backend_choice_per_repo_overrides_top_level() {
        let body = r#"
            [runtime]
            backend = "tmux"

            [runtime.repos.bunyan]
            backend = "zellij"
        "#;
        assert_eq!(
            parse_backend_choice(body, Some("bunyan")),
            Some("zellij".to_string())
        );
        assert_eq!(
            parse_backend_choice(body, Some("other")),
            Some("tmux".to_string())
        );
        assert_eq!(parse_backend_choice(body, None), Some("tmux".to_string()));
    }

    #[test]
    fn parse_backend_choice_ignores_unrelated_sections_and_comments() {
        let body = r#"
            # comment
            [server]
            port = 3333

            [runtime]
            backend = "tmux"  # default for now
        "#;
        assert_eq!(parse_backend_choice(body, None), Some("tmux".to_string()));
    }

    #[test]
    fn parse_backend_choice_returns_none_when_no_runtime_section() {
        let body = "[server]\nport = 3333\n";
        assert_eq!(parse_backend_choice(body, None), None);
    }

    #[test]
    fn backend_by_name_returns_known_backends() {
        assert_eq!(backend_by_name("tmux").unwrap().name(), "tmux");
        assert_eq!(backend_by_name("zellij").unwrap().name(), "zellij");
        assert!(backend_by_name("native").is_none());
        assert!(backend_by_name("bogus").is_none());
    }

    #[test]
    fn is_shell_recognizes_common_shells() {
        assert!(is_shell("zsh"));
        assert!(is_shell("bash"));
        assert!(is_shell("fish"));
        assert!(is_shell("sh"));
        assert!(!is_shell("claude"));
        assert!(!is_shell("2.1.33"));
    }

    #[test]
    fn process_info_to_pane_roundtrips_fields() {
        let info = ProcessInfo {
            handle: "tmux:foo:bar.0".into(),
            command: "claude".into(),
            is_active: true,
            cwd: "/tmp/x".into(),
            pid: 1234,
            slot_index: 0,
        };
        let pane = info.to_pane();
        assert_eq!(pane.pane_index, 0);
        assert_eq!(pane.command, "claude");
        assert!(pane.is_active);
        assert_eq!(pane.workspace_path, "/tmp/x");
        assert_eq!(pane.pane_pid, 1234);
    }
}
