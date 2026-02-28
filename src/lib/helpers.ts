import type { Workspace, WorkspacePaneInfo, TmuxPane, JsonValue } from "@/bindings";
import type { RepoConfig, WorktreeStatus } from "./types";

export const SHELLS = ["zsh", "bash", "fish", "sh"];

export const EDITOR_DISPLAY_NAMES: Record<string, string> = {
  iterm: "iTerm",
  vscode: "VS Code",
  cursor: "Cursor",
  zed: "Zed",
  windsurf: "Windsurf",
  antigravity: "Antigravity",
};

export function asConfig(val: JsonValue | null): RepoConfig | null {
  if (val && typeof val === "object" && !Array.isArray(val)) {
    return val as unknown as RepoConfig;
  }
  return null;
}

export function deriveRepoName(url: string): string {
  const match = url.match(/[/:]([^/:]+?)(?:\.git)?\s*$/);
  return match ? match[1] : "";
}

export function isShellPane(pane: TmuxPane): boolean {
  return SHELLS.includes(pane.command);
}

export function relativeTime(dateStr: string | null): string {
  if (!dateStr) return "";
  const now = Date.now();
  const then = new Date(dateStr).getTime();
  const diffMs = now - then;
  if (diffMs < 0) return "";
  const mins = Math.floor(diffMs / 60000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  const weeks = Math.floor(days / 7);
  return `${weeks}w`;
}

export function getWorktreeStatus(
  workspace: Workspace,
  paneInfo: WorkspacePaneInfo | undefined,
): WorktreeStatus {
  if (workspace.state === "archived") return "archived";
  if (!paneInfo || paneInfo.panes.length === 0) return "idle";
  const hasClaude = paneInfo.panes.some((p) => !isShellPane(p));
  if (hasClaude) return "active";
  return "shell-only";
}

export function consolidateStatus(statuses: WorktreeStatus[]): WorktreeStatus {
  if (statuses.includes("active")) return "active";
  if (statuses.includes("shell-only")) return "shell-only";
  if (statuses.includes("idle")) return "idle";
  return "archived";
}
