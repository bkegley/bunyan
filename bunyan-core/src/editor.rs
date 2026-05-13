use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{BunyanError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Editor {
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
            "vscode" => Some(Editor::Vscode),
            "cursor" => Some(Editor::Cursor),
            "zed" => Some(Editor::Zed),
            "windsurf" => Some(Editor::Windsurf),
            "antigravity" => Some(Editor::Antigravity),
            _ => None,
        }
    }

    /// All editors that can be detected.
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

/// Detect which editors are installed.
///
/// Terminal/multiplexer attachment is no longer an "editor" — that flows
/// through the `workspace.ready_to_view` hook instead.
pub fn detect_installed_editors() -> Vec<Editor> {
    let mut editors = Vec::new();
    for editor in Editor::detectable() {
        if is_cli_available(editor.cli_name()) {
            editors.push(editor.clone());
        }
    }
    editors
}

/// Open a workspace folder in the given editor.
pub fn open_in_editor(editor: &Editor, workspace_path: &str) -> Result<()> {
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
