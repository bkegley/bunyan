use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{BunyanError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Editor {
    Iterm,
    Vscode,
    Cursor,
    Zed,
    Windsurf,
    Antigravity,
}

impl Editor {
    /// The CLI binary name used to open this editor.
    pub fn cli_name(&self) -> &str {
        match self {
            Editor::Iterm => "iterm",
            Editor::Vscode => "code",
            Editor::Cursor => "cursor",
            Editor::Zed => "zed",
            Editor::Windsurf => "windsurf",
            Editor::Antigravity => "agy",
        }
    }

    /// Human-readable display name.
    pub fn display_name(&self) -> &str {
        match self {
            Editor::Iterm => "iTerm",
            Editor::Vscode => "VS Code",
            Editor::Cursor => "Cursor",
            Editor::Zed => "Zed",
            Editor::Windsurf => "Windsurf",
            Editor::Antigravity => "Antigravity",
        }
    }

    /// Stable string identifier used for settings persistence.
    pub fn id(&self) -> &str {
        match self {
            Editor::Iterm => "iterm",
            Editor::Vscode => "vscode",
            Editor::Cursor => "cursor",
            Editor::Zed => "zed",
            Editor::Windsurf => "windsurf",
            Editor::Antigravity => "antigravity",
        }
    }

    /// Parse an editor from its string ID.
    pub fn from_id(id: &str) -> Option<Editor> {
        match id {
            "iterm" => Some(Editor::Iterm),
            "vscode" => Some(Editor::Vscode),
            "cursor" => Some(Editor::Cursor),
            "zed" => Some(Editor::Zed),
            "windsurf" => Some(Editor::Windsurf),
            "antigravity" => Some(Editor::Antigravity),
            _ => None,
        }
    }

    /// All non-iTerm editors that can be detected.
    fn detectable() -> &'static [Editor] {
        &[
            Editor::Vscode,
            Editor::Cursor,
            Editor::Zed,
            Editor::Windsurf,
            Editor::Antigravity,
        ]
    }
}

/// Check if a CLI binary is available on PATH.
fn is_cli_available(cli: &str) -> bool {
    Command::new("which")
        .arg(cli)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Detect which editors are installed. Always includes iTerm as the first entry.
pub fn detect_installed_editors() -> Vec<Editor> {
    let mut editors = vec![Editor::Iterm];
    for editor in Editor::detectable() {
        if is_cli_available(editor.cli_name()) {
            editors.push(editor.clone());
        }
    }
    editors
}

/// Open a workspace folder in the given editor.
/// For iTerm, this is a no-op (handled separately by terminal::attach_iterm).
pub fn open_in_editor(editor: &Editor, workspace_path: &str) -> Result<()> {
    if *editor == Editor::Iterm {
        return Ok(());
    }

    let cli = editor.cli_name();
    let output = Command::new(cli)
        .arg(workspace_path)
        .output()
        .map_err(|e| {
            BunyanError::Process(format!("Failed to launch {}: {}", editor.display_name(), e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BunyanError::Process(format!(
            "{} exited with error: {}",
            editor.display_name(),
            stderr
        )));
    }

    Ok(())
}
