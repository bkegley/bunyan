import type {
  Repo,
  Workspace,
  JsonValue,
  ClaudeSessionEntry,
  WorkspacePaneInfo,
  ContainerMode,
} from "@/bindings";

export interface ContainerConfig {
  enabled: boolean;
  image?: string;
  dangerously_skip_permissions?: boolean;
}

export interface RepoConfig {
  scripts?: { setup?: string; run?: string };
  runScriptMode?: string;
  container?: ContainerConfig;
}

export type WorktreeStatus = "active" | "shell-only" | "idle" | "archived";

export interface AppContextType {
  repos: Repo[];
  workspaces: Workspace[];
  workspacePanes: Map<string, WorkspacePaneInfo>;
  showArchived: boolean;
  expandedRepos: Set<string>;
  selectedWorktreeId: string | null;
  openingSession: Set<string>;
  homePath: string;
  dockerAvailable: boolean;
  selectedSessions: ClaudeSessionEntry[];
  loadingSessions: boolean;

  setShowArchived: (show: boolean) => void;
  toggleRepo: (id: string) => void;
  selectWorktree: (id: string) => void;
  createRepo: (name: string, remoteUrl: string) => Promise<void>;
  updateRepoSettings: (repoId: string, config: JsonValue | null) => Promise<void>;
  deleteRepoById: (id: string) => Promise<void>;
  createNewWorkspace: (
    repoId: string,
    name: string,
    branch: string,
    containerMode?: ContainerMode,
  ) => Promise<void>;
  archiveWorkspaceById: (id: string, force?: boolean) => Promise<void>;
  confirmArchive: (workspace: Workspace) => void;
  openClaude: (workspaceId: string) => Promise<void>;
  openShell: (workspaceId: string) => Promise<void>;
  viewWorkspace: (workspaceId: string) => Promise<void>;
  openInEditor: (workspaceId: string, editorId?: string) => Promise<void>;
  killPane: (workspaceId: string, paneIndex: number) => Promise<void>;
  resumeSession: (workspaceId: string, sessionId: string) => Promise<void>;
  detectedEditors: string[];
  preferredEditor: string;
}
