//! Zellij backend.
//!
//! Mapping vs. the `RuntimeBackend` trait:
//!   repo → zellij session
//!   workspace → tab in that session
//!   process slot → pane in that tab
//!
//! Zellij's CLI has rougher edges than tmux for the things bunyan needs:
//!
//! - No PID or per-pane CWD exposed in `list-panes --json`. Bunyan stores
//!   what it can (command, exited flag, pane title) and leaves `pid`/`cwd`
//!   as best-effort empty.
//! - Panes are identified by string IDs (`terminal_<n>`), not slot indices.
//!   Bunyan keeps a stable mapping by hashing the pane id into a `u32`
//!   slot. Different bunyan invocations see the same id → same slot.
//! - There's no `list-panes -a` equivalent — `list_all_processes` walks
//!   `list-sessions` then `list-panes` per session.
//! - There's no shipped way to inspect which pane runs which Claude session
//!   ID, so `find_slot_running_session` always returns `Ok(None)`. The
//!   default trait helper's PID-tree introspection doesn't apply here.
//!
//! Requires zellij >= 0.44 (stable `--create-background` and `--json`).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;

use super::{ProcessHandle, ProcessInfo, RuntimeBackend};
use crate::error::{BunyanError, Result};

#[derive(Default)]
pub struct ZellijBackend;

impl ZellijBackend {
    pub fn new() -> Self {
        Self
    }
}

fn zellij_cmd() -> Command {
    Command::new("zellij")
}

fn session_exists(repo_name: &str) -> bool {
    let out = match zellij_cmd().args(["list-sessions", "--short", "--no-formatting"]).output() {
        Ok(o) => o,
        Err(_) => return false,
    };
    if !out.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .any(|line| line.trim_end_matches(" (current)") == repo_name)
}

fn slot_for_pane_id(pane_id: &str) -> u32 {
    let mut hasher = DefaultHasher::new();
    pane_id.hash(&mut hasher);
    // Truncate to u32, keep stable across runs.
    (hasher.finish() & 0xFFFFFFFF) as u32
}

fn make_handle(repo: &str, workspace: &str, pane_id: &str) -> ProcessHandle {
    format!("zellij:{}/{}:{}", repo, workspace, pane_id)
}

fn run_zellij_action(repo_name: &str, args: &[&str]) -> Result<()> {
    let mut all = vec!["--session", repo_name, "action"];
    all.extend_from_slice(args);
    let out = zellij_cmd()
        .args(&all)
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to run zellij action: {}", e)))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(BunyanError::Process(format!(
            "zellij action {:?} failed: {}",
            args, stderr
        )));
    }
    Ok(())
}

fn focus_pane(repo_name: &str, pane_id: u32) -> Result<()> {
    run_zellij_action(repo_name, &["focus-pane-id", &pane_id.to_string()])
}

#[derive(Debug, Default)]
struct TabRef {
    /// 1-indexed tab id within the session.
    id: u32,
    /// Tab name (the workspace's directory_name in bunyan terms).
    name: String,
}

fn list_tabs(repo_name: &str) -> Result<Vec<TabRef>> {
    let out = zellij_cmd()
        .args(["--session", repo_name, "action", "list-tabs", "--json"])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to list zellij tabs: {}", e)))?;
    if !out.status.success() {
        return Ok(vec![]);
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    parse_tabs_json(&stdout)
}

fn parse_tabs_json(input: &str) -> Result<Vec<TabRef>> {
    let v: serde_json::Value = serde_json::from_str(input.trim()).unwrap_or(serde_json::Value::Null);
    let arr = match v.as_array() {
        Some(a) => a,
        None => return Ok(vec![]),
    };
    let mut tabs = Vec::new();
    for item in arr {
        // Zellij 0.44 exposes both `tab_id` and `position` (both 0-indexed
        // and identical in practice). Use `tab_id` as the addressable id.
        let id = item
            .get("tab_id")
            .or_else(|| item.get("position"))
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(id) = id {
            tabs.push(TabRef { id, name });
        }
    }
    Ok(tabs)
}

fn find_tab_id(repo_name: &str, workspace_name: &str) -> Result<Option<u32>> {
    Ok(list_tabs(repo_name)?
        .into_iter()
        .find(|t| t.name == workspace_name)
        .map(|t| t.id))
}

fn list_panes_for_tab(repo_name: &str, tab_id: u32) -> Result<Vec<ProcessInfo>> {
    let out = zellij_cmd()
        .args(["--session", repo_name, "action", "list-panes", "--json"])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to list zellij panes: {}", e)))?;
    if !out.status.success() {
        return Ok(vec![]);
    }
    // list-panes returns every pane in every tab; filter by tab_id here.
    parse_panes_json(&String::from_utf8_lossy(&out.stdout), Some(tab_id))
}

fn parse_panes_json(input: &str, filter_tab_id: Option<u32>) -> Result<Vec<ProcessInfo>> {
    let v: serde_json::Value = serde_json::from_str(input.trim()).unwrap_or(serde_json::Value::Null);
    let arr = match v.as_array() {
        Some(a) => a,
        None => return Ok(vec![]),
    };
    let mut panes = Vec::new();
    for item in arr {
        // Zellij returns `id` as an integer scoped to its tab. Skip plugin panes.
        if item
            .get("is_plugin")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let pane_id = match item.get("id").and_then(|v| v.as_u64()) {
            Some(n) => n as u32,
            None => continue,
        };
        let item_tab_id = item.get("tab_id").and_then(|v| v.as_u64()).map(|n| n as u32);
        if let (Some(want), Some(have)) = (filter_tab_id, item_tab_id) {
            if want != have {
                continue;
            }
        }
        // `terminal_command` is the most accurate source — but it's often
        // null for shells. Fall back to title (which Zellij uses as
        // "Pane #N" by default until the running process sets it).
        let cmd = item
            .get("terminal_command")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("title").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        let is_active = item
            .get("is_focused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let cwd = item
            .get("pane_cwd")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // Slot index is a stable hash of "<tab_id>:<pane_id>" so the same
        // pane returns the same slot across calls.
        let slot_key = format!("{}:{}", item_tab_id.unwrap_or(0), pane_id);
        panes.push(ProcessInfo {
            handle: String::new(),
            command: extract_first_word(&cmd),
            is_active,
            cwd,
            pid: 0,
            slot_index: slot_for_pane_id(&slot_key),
        });
    }
    Ok(panes)
}

fn extract_first_word(s: &str) -> String {
    s.split_whitespace().next().unwrap_or("").to_string()
}

/// Find the (tab_id, pane_id) pair that hashes to the given slot_index.
/// Returns None if no matching pane is currently alive.
fn find_pane_for_slot(repo_name: &str, slot_index: u32) -> Result<Option<(u32, u32)>> {
    let out = zellij_cmd()
        .args(["--session", repo_name, "action", "list-panes", "--json"])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to list panes: {}", e)))?;
    if !out.status.success() {
        return Ok(None);
    }
    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout))
        .unwrap_or(serde_json::Value::Null);
    let arr = match v.as_array() {
        Some(a) => a,
        None => return Ok(None),
    };
    for item in arr {
        let pane_id = match item.get("id").and_then(|v| v.as_u64()) {
            Some(n) => n as u32,
            None => continue,
        };
        let tab_id = item.get("tab_id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if slot_for_pane_id(&format!("{}:{}", tab_id, pane_id)) == slot_index {
            return Ok(Some((tab_id, pane_id)));
        }
    }
    Ok(None)
}

impl RuntimeBackend for ZellijBackend {
    fn name(&self) -> &'static str {
        "zellij"
    }

    fn ensure_workspace(
        &self,
        repo_name: &str,
        workspace_name: &str,
        workspace_path: &str,
    ) -> Result<()> {
        if !session_exists(repo_name) {
            let out = zellij_cmd()
                .args(["attach", "--create-background", repo_name])
                .output()
                .map_err(|e| {
                    BunyanError::Process(format!("Failed to create zellij session: {}", e))
                })?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(BunyanError::Process(format!(
                    "zellij attach --create-background failed: {}",
                    stderr
                )));
            }
        }

        if find_tab_id(repo_name, workspace_name)?.is_none() {
            let out = zellij_cmd()
                .args([
                    "--session",
                    repo_name,
                    "action",
                    "new-tab",
                    "--name",
                    workspace_name,
                    "--cwd",
                    workspace_path,
                ])
                .output()
                .map_err(|e| BunyanError::Process(format!("Failed to create zellij tab: {}", e)))?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(BunyanError::Process(format!(
                    "zellij new-tab failed: {}",
                    stderr
                )));
            }
        }

        Ok(())
    }

    fn kill_workspace(&self, repo_name: &str, workspace_name: &str) -> Result<()> {
        // Close just the tab rather than the whole session: a session may
        // be shared across multiple workspaces.
        if let Some(tab_id) = find_tab_id(repo_name, workspace_name)? {
            let _ = zellij_cmd()
                .args([
                    "--session",
                    repo_name,
                    "action",
                    "close-tab",
                    "--tab-id",
                    &tab_id.to_string(),
                ])
                .output();
        }
        Ok(())
    }

    fn list_processes(
        &self,
        repo_name: &str,
        workspace_name: &str,
    ) -> Result<Vec<ProcessInfo>> {
        let tab_id = match find_tab_id(repo_name, workspace_name)? {
            Some(id) => id,
            None => return Ok(vec![]),
        };
        let mut panes = list_panes_for_tab(repo_name, tab_id)?;
        for p in panes.iter_mut() {
            // Reconstruct the handle now that we know the workspace.
            // We can't recover the original pane_id without another lookup,
            // so we use a slot-shaped handle.
            p.handle = make_handle(repo_name, workspace_name, &format!("slot_{}", p.slot_index));
        }
        Ok(panes)
    }

    fn list_all_processes(&self) -> Result<Vec<(String, String, ProcessInfo)>> {
        let sessions_out = zellij_cmd()
            .args(["list-sessions", "--short", "--no-formatting"])
            .output()
            .map_err(|e| BunyanError::Process(format!("Failed to list zellij sessions: {}", e)))?;
        if !sessions_out.status.success() {
            return Ok(vec![]);
        }
        let stdout = String::from_utf8_lossy(&sessions_out.stdout);
        let mut all = Vec::new();
        for line in stdout.lines() {
            let name = line.trim_end_matches(" (current)").trim().to_string();
            if name.is_empty() {
                continue;
            }
            let tabs = match list_tabs(&name) {
                Ok(t) => t,
                Err(_) => continue,
            };
            for tab in tabs {
                let panes = match list_panes_for_tab(&name, tab.id) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                for mut p in panes {
                    p.handle =
                        make_handle(&name, &tab.name, &format!("slot_{}", p.slot_index));
                    all.push((name.clone(), tab.name.clone(), p));
                }
            }
        }
        Ok(all)
    }

    fn spawn(
        &self,
        repo_name: &str,
        workspace_name: &str,
        workspace_path: &str,
        cmd: &str,
    ) -> Result<()> {
        self.ensure_workspace(repo_name, workspace_name, workspace_path)?;
        let tab_id = find_tab_id(repo_name, workspace_name)?
            .ok_or_else(|| BunyanError::Process("zellij tab missing after creation".into()))?;

        // Run the command as `sh -c "<cmd>"` so shell features (pipes, env
        // vars) work — bunyan callers compose shell strings, not argv.
        let out = zellij_cmd()
            .args([
                "--session",
                repo_name,
                "action",
                "new-pane",
                "--tab-id",
                &tab_id.to_string(),
                "--cwd",
                workspace_path,
                "--",
                "sh",
                "-c",
                cmd,
            ])
            .output()
            .map_err(|e| BunyanError::Process(format!("Failed to create zellij pane: {}", e)))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(BunyanError::Process(format!(
                "zellij new-pane failed: {}",
                stderr
            )));
        }
        Ok(())
    }

    fn send_to_slot(
        &self,
        repo_name: &str,
        _workspace_name: &str,
        slot_index: u32,
        cmd: &str,
    ) -> Result<()> {
        let (_, pane_id) = find_pane_for_slot(repo_name, slot_index)?
            .ok_or_else(|| BunyanError::NotFound("zellij pane not found".into()))?;
        // zellij's write/write-chars target the focused pane; focus first.
        focus_pane(repo_name, pane_id)?;
        // write-chars then Enter (Enter = 13).
        run_zellij_action(repo_name, &["write-chars", cmd])?;
        run_zellij_action(repo_name, &["write", "13"])?;
        Ok(())
    }

    fn kill_slot(&self, repo_name: &str, _workspace_name: &str, slot_index: u32) -> Result<()> {
        let (_, pane_id) = find_pane_for_slot(repo_name, slot_index)?
            .ok_or_else(|| BunyanError::NotFound("zellij pane not found".into()))?;
        focus_pane(repo_name, pane_id)?;
        run_zellij_action(repo_name, &["close-pane"])?;
        Ok(())
    }

    fn attach_command(&self, repo_name: &str) -> String {
        format!("zellij attach {}", repo_name)
    }

    /// Zellij doesn't expose pane PIDs, so we can't introspect a pane's
    /// running Claude session ID. Always return None — `bunyan-review`
    /// users on zellij rely on tab/pane names + diffs instead.
    fn find_slot_running_session(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Option<u32>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_command_returns_zellij_attach_form() {
        assert_eq!(
            ZellijBackend::new().attach_command("frontend"),
            "zellij attach frontend"
        );
    }

    #[test]
    fn name_is_zellij() {
        assert_eq!(ZellijBackend::new().name(), "zellij");
    }

    #[test]
    fn slot_for_pane_id_is_stable_for_same_id() {
        let a = slot_for_pane_id("terminal_5");
        let b = slot_for_pane_id("terminal_5");
        assert_eq!(a, b);
    }

    #[test]
    fn slot_for_pane_id_differs_for_different_ids() {
        let a = slot_for_pane_id("terminal_5");
        let b = slot_for_pane_id("terminal_6");
        assert_ne!(a, b);
    }

    #[test]
    fn parse_tabs_json_reads_tab_id_field() {
        let json = r#"[
            {"position":0,"name":"first","tab_id":0},
            {"position":1,"name":"second","tab_id":1}
        ]"#;
        let tabs = parse_tabs_json(json).unwrap();
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].id, 0);
        assert_eq!(tabs[0].name, "first");
        assert_eq!(tabs[1].id, 1);
        assert_eq!(tabs[1].name, "second");
    }

    #[test]
    fn parse_tabs_json_returns_empty_on_malformed_input() {
        assert!(parse_tabs_json("not json").unwrap().is_empty());
        assert!(parse_tabs_json("").unwrap().is_empty());
    }

    #[test]
    fn parse_panes_json_extracts_command_and_id() {
        let json = r#"[
            {"id":2,"tab_id":1,"title":"claude","terminal_command":"claude --resume foo","is_focused":true,"pane_cwd":"/work","is_plugin":false},
            {"id":1,"tab_id":1,"title":"zsh","terminal_command":null,"is_focused":false,"pane_cwd":"/work","is_plugin":false}
        ]"#;
        let panes = parse_panes_json(json, None).unwrap();
        assert_eq!(panes.len(), 2);
        // terminal_command preferred over title
        assert_eq!(panes[0].command, "claude");
        assert!(panes[0].is_active);
        // falls back to title when terminal_command is null
        assert_eq!(panes[1].command, "zsh");
        assert!(!panes[1].is_active);
        assert_eq!(panes[0].cwd, "/work");
        // slot_index hashes "<tab_id>:<pane_id>"
        assert_eq!(panes[0].slot_index, slot_for_pane_id("1:2"));
        assert_eq!(panes[1].slot_index, slot_for_pane_id("1:1"));
    }

    #[test]
    fn parse_panes_json_filters_by_tab_id() {
        let json = r#"[
            {"id":0,"tab_id":0,"title":"a","is_plugin":false},
            {"id":1,"tab_id":1,"title":"b","is_plugin":false},
            {"id":2,"tab_id":1,"title":"c","is_plugin":false}
        ]"#;
        let panes = parse_panes_json(json, Some(1)).unwrap();
        assert_eq!(panes.len(), 2);
    }

    #[test]
    fn parse_panes_json_skips_plugin_panes() {
        let json = r#"[
            {"id":0,"tab_id":0,"title":"a","is_plugin":true},
            {"id":1,"tab_id":0,"title":"b","is_plugin":false}
        ]"#;
        let panes = parse_panes_json(json, None).unwrap();
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].command, "b");
    }

    #[test]
    fn extract_first_word_strips_args() {
        assert_eq!(extract_first_word("claude --resume abc"), "claude");
        assert_eq!(extract_first_word("zsh"), "zsh");
        assert_eq!(extract_first_word(""), "");
    }

    #[test]
    fn find_slot_running_session_always_none() {
        let backend = ZellijBackend::new();
        assert_eq!(
            backend.find_slot_running_session("r", "w", "session-id").unwrap(),
            None
        );
    }
}
