import {
  useState,
  useEffect,
  useCallback,
  createContext,
  useContext,
} from "react";
import { homeDir } from "@tauri-apps/api/path";
import {
  commands,
  type Repo,
  type Workspace,
  type JsonValue,
  type ClaudeSessionEntry,
} from "./bindings";
import "./App.css";

// ---------------------------------------------------------------------------
// Local types for conductor_config (JsonValue from bindings is generic)
// ---------------------------------------------------------------------------

interface ConductorConfig {
  scripts?: { setup?: string; run?: string };
  runScriptMode?: string;
}

function asConfig(val: JsonValue | null): ConductorConfig | null {
  if (val && typeof val === "object" && !Array.isArray(val)) {
    return val as unknown as ConductorConfig;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

interface AppContextType {
  repos: Repo[];
  workspaces: Workspace[];
  activeSessions: Set<string>;
  otherSessionCount: number;
  showArchived: boolean;
  expandedRepos: Set<string>;
  openSettingsRepo: string | null;
  newWorktreeRepo: string | null;
  creatingWorkspace: Set<string>;
  archivingWorkspace: Set<string>;
  openingSession: Set<string>;
  homePath: string;
  cloningRepo: boolean;
  cloneError: string | null;

  setCurrentView: (view: "main" | "addRepo") => void;
  setShowArchived: (show: boolean) => void;
  toggleRepo: (id: string) => void;
  setOpenSettingsRepo: (id: string | null) => void;
  setNewWorktreeRepo: (id: string | null) => void;
  setCloneError: (err: string | null) => void;
  createRepo: (name: string, remoteUrl: string) => Promise<void>;
  updateRepoSettings: (
    repoId: string,
    config: JsonValue | null,
  ) => Promise<void>;
  deleteRepoById: (id: string) => Promise<void>;
  createNewWorkspace: (
    repoId: string,
    name: string,
    branch: string,
  ) => Promise<void>;
  archiveWorkspaceById: (id: string) => Promise<void>;
  openClaude: (workspaceId: string) => Promise<void>;
  expandedWorkspaceSessions: Set<string>;
  workspaceSessions: Map<string, ClaudeSessionEntry[]>;
  toggleWorkspaceSessions: (workspaceId: string) => void;
  resumeSession: (workspaceId: string, sessionId: string) => Promise<void>;
}

const AppContext = createContext<AppContextType>(null!);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function deriveRepoName(url: string): string {
  const match = url.match(/[/:]([^/:]+?)(?:\.git)?\s*$/);
  return match ? match[1] : "";
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function MainView() {
  const ctx = useContext(AppContext);

  return (
    <div className="app">
      <header className="header">
        <div className="header-title">Bunyan</div>
        <div className="header-actions">
          <label>
            <input
              type="checkbox"
              checked={ctx.showArchived}
              onChange={(e) => ctx.setShowArchived(e.target.checked)}
            />
            Show archived
          </label>
          <button
            className="btn btn-primary btn-sm"
            onClick={() => ctx.setCurrentView("addRepo")}
          >
            + Repo
          </button>
        </div>
      </header>

      <div className="main-content">
        {ctx.repos.length === 0 ? (
          <div className="empty-state">
            No repositories yet. Click &quot;+ Repo&quot; to add one.
          </div>
        ) : (
          ctx.repos.map((repo) => <RepoSection key={repo.id} repo={repo} />)
        )}
      </div>

      {ctx.otherSessionCount > 0 && (
        <div className="footer">
          Other claude sessions: {ctx.otherSessionCount}
        </div>
      )}
    </div>
  );
}

function RepoSection({ repo }: { repo: Repo }) {
  const ctx = useContext(AppContext);
  const isExpanded = ctx.expandedRepos.has(repo.id);
  const isSettingsOpen = ctx.openSettingsRepo === repo.id;
  const isNewWorktreeOpen = ctx.newWorktreeRepo === repo.id;

  const repoWorkspaces = ctx.workspaces.filter(
    (ws) =>
      ws.repository_id === repo.id &&
      (ctx.showArchived || ws.state === "ready"),
  );

  const handleGearClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    ctx.setOpenSettingsRepo(isSettingsOpen ? null : repo.id);
  };

  return (
    <div className="repo-section">
      <div className="repo-header" onClick={() => ctx.toggleRepo(repo.id)}>
        <div className="repo-header-left">
          <span className={`chevron ${isExpanded ? "expanded" : ""}`}>
            &#9654;
          </span>
          <span>{repo.name}</span>
        </div>
        <button
          className="gear-btn"
          onClick={handleGearClick}
          title="Settings"
        >
          &#9881;
        </button>
      </div>

      {isExpanded && (
        <div className="repo-content">
          {isSettingsOpen && <RepoSettingsPanel repo={repo} />}

          <div className="workspace-list">
            {repoWorkspaces.map((ws) => (
              <WorkspaceRow key={ws.id} workspace={ws} />
            ))}
            {ctx.creatingWorkspace.has(repo.id) && (
              <div className="workspace-row creating">
                <span className="workspace-name">Creating...</span>
                <span className="workspace-branch" />
                <div className="workspace-actions">
                  <span className="spinner spinner-sm" />
                </div>
              </div>
            )}
          </div>

          {isNewWorktreeOpen ? (
            <NewWorktreeForm repo={repo} />
          ) : (
            <button
              className="new-worktree-btn"
              onClick={() => ctx.setNewWorktreeRepo(repo.id)}
            >
              + New Worktree
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function RepoSettingsPanel({ repo }: { repo: Repo }) {
  const ctx = useContext(AppContext);
  const config = asConfig(repo.conductor_config);
  const [setupScript, setSetupScript] = useState(
    config?.scripts?.setup ?? "",
  );
  const [runScript, setRunScript] = useState(config?.scripts?.run ?? "");
  const [saving, setSaving] = useState(false);

  const hasChanges =
    setupScript !== (config?.scripts?.setup ?? "") ||
    runScript !== (config?.scripts?.run ?? "");

  const handleSave = async () => {
    setSaving(true);
    try {
      const newConfig: ConductorConfig = {
        scripts: {
          ...(setupScript ? { setup: setupScript } : {}),
          ...(runScript ? { run: runScript } : {}),
        },
        ...(config?.runScriptMode
          ? { runScriptMode: config.runScriptMode }
          : {}),
      };
      await ctx.updateRepoSettings(
        repo.id,
        newConfig as unknown as JsonValue,
      );
      ctx.setOpenSettingsRepo(null);
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (
      !window.confirm(
        `Delete ${repo.name}? This will remove the repo and all its workspaces from the database.`,
      )
    ) {
      return;
    }
    await ctx.deleteRepoById(repo.id);
  };

  return (
    <div className="settings-panel">
      <div className="form-group">
        <label>Setup script</label>
        <input
          type="text"
          value={setupScript}
          onChange={(e) => setSetupScript(e.target.value)}
          placeholder="e.g. mise trust && make setup"
        />
      </div>
      <div className="form-group">
        <label>Run script</label>
        <input
          type="text"
          value={runScript}
          onChange={(e) => setRunScript(e.target.value)}
          placeholder="e.g. npm run dev"
        />
      </div>
      <div className="settings-actions">
        <button
          className="btn btn-primary btn-sm"
          onClick={handleSave}
          disabled={!hasChanges || saving}
        >
          {saving ? "Saving..." : "Save"}
        </button>
        <button
          className="btn btn-sm"
          onClick={() => ctx.setOpenSettingsRepo(null)}
        >
          Cancel
        </button>
      </div>
      <div className="settings-danger">
        <button className="btn btn-danger btn-sm" onClick={handleDelete}>
          Delete repo
        </button>
      </div>
    </div>
  );
}

function WorkspaceRow({ workspace }: { workspace: Workspace }) {
  const ctx = useContext(AppContext);
  const isActive = ctx.activeSessions.has(workspace.id);
  const isArchived = workspace.state === "archived";
  const isArchiving = ctx.archivingWorkspace.has(workspace.id);
  const isOpening = ctx.openingSession.has(workspace.id);
  const isSessionsExpanded = ctx.expandedWorkspaceSessions.has(workspace.id);
  const sessions = ctx.workspaceSessions.get(workspace.id) ?? [];

  const handleClaude = async () => {
    await ctx.openClaude(workspace.id);
  };

  const handleArchive = async () => {
    if (
      !window.confirm(
        `Archive ${workspace.directory_name}? This will remove the worktree from disk.`,
      )
    ) {
      return;
    }
    await ctx.archiveWorkspaceById(workspace.id);
  };

  return (
    <div className={`workspace-row-container ${isArchived ? "archived" : ""}`}>
      <div className="workspace-row">
        <span className="workspace-name">{workspace.directory_name}</span>
        <span className="workspace-branch">{workspace.branch}</span>
        {isArchived ? (
          <span className="archived-badge">archived</span>
        ) : (
          <div className="workspace-actions">
            <button
              className="sessions-toggle-btn"
              onClick={() => ctx.toggleWorkspaceSessions(workspace.id)}
            >
              {isSessionsExpanded ? "\u25be" : "\u25b8"} Sessions
            </button>
            <button
              className="claude-btn"
              onClick={handleClaude}
              disabled={isOpening}
            >
              {isOpening ? (
                <span className="spinner spinner-sm" />
              ) : (
                <span
                  className={`claude-dot ${isActive ? "active" : "inactive"}`}
                />
              )}
              Claude
            </button>
            <button
              className="archive-btn"
              onClick={handleArchive}
              disabled={isArchiving}
            >
              {isArchiving ? "..." : "Archive"}
            </button>
          </div>
        )}
      </div>

      {isSessionsExpanded && !isArchived && (
        <div className="session-list">
          {sessions.length === 0 ? (
            <div className="session-empty">No sessions yet</div>
          ) : (
            sessions.map((s) => (
              <SessionRow
                key={s.session_id}
                session={s}
                workspaceId={workspace.id}
              />
            ))
          )}
        </div>
      )}
    </div>
  );
}

function SessionRow({
  session,
  workspaceId,
}: {
  session: ClaudeSessionEntry;
  workspaceId: string;
}) {
  const ctx = useContext(AppContext);

  const handleResume = async () => {
    await ctx.resumeSession(workspaceId, session.session_id);
  };

  const snippet = session.first_prompt
    ? session.first_prompt.length > 80
      ? session.first_prompt.slice(0, 80) + "..."
      : session.first_prompt
    : "(no prompt)";

  const dateStr = session.created
    ? new Date(session.created).toLocaleDateString()
    : "";

  return (
    <div className="session-row" onClick={handleResume} title="Resume this session">
      <span className="session-prompt">{snippet}</span>
      <span className="session-meta">
        {session.message_count ?? 0} msgs
        {dateStr && ` \u00b7 ${dateStr}`}
      </span>
    </div>
  );
}

function NewWorktreeForm({ repo }: { repo: Repo }) {
  const ctx = useContext(AppContext);
  const [name, setName] = useState("");
  const [baseBranch, setBaseBranch] = useState(repo.default_branch);
  const [error, setError] = useState("");

  const validate = (value: string): string => {
    if (!value.trim()) return "Name is required";
    if (/\s/.test(value)) return "No spaces allowed";
    const existing = ctx.workspaces.filter(
      (ws) => ws.repository_id === repo.id,
    );
    if (existing.some((ws) => ws.directory_name === value))
      return "Name already exists";
    return "";
  };

  const handleCreate = async () => {
    const err = validate(name);
    if (err) {
      setError(err);
      return;
    }
    ctx.setNewWorktreeRepo(null);
    await ctx.createNewWorkspace(repo.id, name, name);
  };

  return (
    <div className="new-worktree-form">
      <div className="form-row">
        <input
          type="text"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            setError("");
          }}
          placeholder="Worktree name"
          autoFocus
        />
        <input
          type="text"
          value={baseBranch}
          onChange={(e) => setBaseBranch(e.target.value)}
          placeholder="Base branch"
          style={{ maxWidth: 140 }}
        />
      </div>
      {error && <div className="validation-error">{error}</div>}
      <div className="form-actions">
        <button className="btn btn-primary btn-sm" onClick={handleCreate}>
          Create
        </button>
        <button
          className="btn btn-sm"
          onClick={() => ctx.setNewWorktreeRepo(null)}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

function AddRepoView() {
  const ctx = useContext(AppContext);
  const [url, setUrl] = useState("");
  const derivedName = deriveRepoName(url);

  const handleClone = async () => {
    ctx.setCloneError(null);
    await ctx.createRepo(derivedName, url);
  };

  const handleBack = () => {
    ctx.setCloneError(null);
    ctx.setCurrentView("main");
  };

  return (
    <div className="add-repo-view">
      <div className="add-repo-header">
        <button className="back-btn" onClick={handleBack}>
          &larr; Back
        </button>
      </div>
      <div className="add-repo-content">
        <h2>Add Repository</h2>

        {ctx.cloningRepo ? (
          <div className="clone-loading">
            <span className="spinner" />
            <span>Cloning {derivedName}...</span>
          </div>
        ) : (
          <>
            <div className="form-group">
              <label>Remote URL</label>
              <input
                type="text"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="git@github.com:org/repo.git"
                autoFocus
              />
            </div>
            <div className="form-group">
              <label>Repository name (derived from URL)</label>
              <input type="text" value={derivedName} readOnly />
            </div>
            <div className="form-actions">
              <button className="btn" onClick={handleBack}>
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleClone}
                disabled={!url.trim() || !derivedName}
              >
                Clone Repo
              </button>
            </div>
          </>
        )}

        {ctx.cloneError && (
          <div className="error-message">{ctx.cloneError}</div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// App (root)
// ---------------------------------------------------------------------------

function App() {
  // Data
  const [repos, setRepos] = useState<Repo[]>([]);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [activeSessions, setActiveSessions] = useState<Set<string>>(
    new Set(),
  );
  const [otherSessionCount, setOtherSessionCount] = useState(0);
  const [homePath, setHomePath] = useState("");

  // UI state
  const [currentView, setCurrentView] = useState<"main" | "addRepo">("main");
  const [expandedRepos, setExpandedRepos] = useState<Set<string>>(new Set());
  const [openSettingsRepo, setOpenSettingsRepo] = useState<string | null>(
    null,
  );
  const [showArchived, setShowArchived] = useState(false);
  const [newWorktreeRepo, setNewWorktreeRepo] = useState<string | null>(null);

  // Loading states
  const [cloningRepo, setCloningRepo] = useState(false);
  const [cloneError, setCloneError] = useState<string | null>(null);
  const [creatingWorkspace, setCreatingWorkspace] = useState<Set<string>>(
    new Set(),
  );
  const [archivingWorkspace, setArchivingWorkspace] = useState<Set<string>>(
    new Set(),
  );
  const [openingSession, setOpeningSession] = useState<Set<string>>(
    new Set(),
  );

  // Session list state
  const [expandedWorkspaceSessions, setExpandedWorkspaceSessions] = useState<
    Set<string>
  >(new Set());
  const [workspaceSessions, setWorkspaceSessions] = useState<
    Map<string, ClaudeSessionEntry[]>
  >(new Map());

  // ---- Session polling ----

  const pollSessions = useCallback(async () => {
    try {
      const sessions = await commands.getActiveClaudeSessions();
      const activeIds = new Set<string>();
      let otherCount = 0;
      for (const s of sessions) {
        if (s.workspace_id) {
          activeIds.add(s.workspace_id);
        } else {
          otherCount++;
        }
      }
      setActiveSessions(activeIds);
      setOtherSessionCount(otherCount);
    } catch {
      // Fail silently — just show no green dots
    }
  }, []);

  // ---- Initial data load ----

  useEffect(() => {
    (async () => {
      try {
        const [repoList, wsList, home] = await Promise.all([
          commands.listRepos(),
          commands.listWorkspaces(null),
          homeDir(),
        ]);
        setRepos(repoList);
        setWorkspaces(wsList);
        setExpandedRepos(new Set(repoList.map((r) => r.id)));
        setHomePath(home.replace(/\/$/, ""));
      } catch (e) {
        console.error("Failed to load initial data:", e);
      }
      pollSessions();
    })();
  }, [pollSessions]);

  // ---- Polling interval + focus ----

  useEffect(() => {
    const interval = setInterval(pollSessions, 5000);

    const handleFocus = () => {
      pollSessions();
    };
    const handleVisibility = () => {
      if (!document.hidden) pollSessions();
    };

    window.addEventListener("focus", handleFocus);
    document.addEventListener("visibilitychange", handleVisibility);

    return () => {
      clearInterval(interval);
      window.removeEventListener("focus", handleFocus);
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, [pollSessions]);

  // ---- Action handlers ----

  const toggleRepo = useCallback((id: string) => {
    setExpandedRepos((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleCreateRepo = useCallback(
    async (name: string, remoteUrl: string) => {
      setCloningRepo(true);
      setCloneError(null);
      try {
        const rootPath = `${homePath}/bunyan/repos/${name}`;
        const repo = await commands.createRepo({
          name,
          remote_url: remoteUrl,
          root_path: rootPath,
          default_branch: "main",
          remote: "origin",
          display_order: 0,
          conductor_config: null,
        });
        setRepos((prev) => [...prev, repo]);
        setExpandedRepos((prev) => new Set([...prev, repo.id]));
        setCurrentView("main");
      } catch (e) {
        setCloneError(String(e));
      } finally {
        setCloningRepo(false);
      }
    },
    [homePath],
  );

  const handleUpdateRepoSettings = useCallback(
    async (repoId: string, config: JsonValue | null) => {
      const repo = await commands.updateRepo({
        id: repoId,
        name: null,
        default_branch: null,
        display_order: null,
        conductor_config: config,
      });
      setRepos((prev) => prev.map((r) => (r.id === repo.id ? repo : r)));
    },
    [],
  );

  const handleDeleteRepo = useCallback(async (id: string) => {
    await commands.deleteRepo(id);
    setRepos((prev) => prev.filter((r) => r.id !== id));
    setWorkspaces((prev) => prev.filter((ws) => ws.repository_id !== id));
    setOpenSettingsRepo(null);
  }, []);

  const handleCreateWorkspace = useCallback(
    async (repoId: string, name: string, branch: string) => {
      setCreatingWorkspace((prev) => new Set([...prev, repoId]));
      try {
        const ws = await commands.createWorkspace({
          repository_id: repoId,
          directory_name: name,
          branch,
        });
        setWorkspaces((prev) => [...prev, ws]);
      } finally {
        setCreatingWorkspace((prev) => {
          const next = new Set(prev);
          next.delete(repoId);
          return next;
        });
      }
    },
    [],
  );

  const handleArchiveWorkspace = useCallback(async (id: string) => {
    setArchivingWorkspace((prev) => new Set([...prev, id]));
    try {
      const updated = await commands.archiveWorkspace(id);
      setWorkspaces((prev) =>
        prev.map((ws) => (ws.id === updated.id ? updated : ws)),
      );
    } finally {
      setArchivingWorkspace((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  }, []);

  const handleOpenClaude = useCallback(
    async (workspaceId: string) => {
      setOpeningSession((prev) => new Set([...prev, workspaceId]));
      try {
        await commands.openClaudeSession(workspaceId);
      } finally {
        setOpeningSession((prev) => {
          const next = new Set(prev);
          next.delete(workspaceId);
          return next;
        });
        setTimeout(pollSessions, 1000);
      }
    },
    [pollSessions],
  );

  // ---- Session list handlers ----

  const toggleWorkspaceSessions = useCallback(
    async (workspaceId: string) => {
      setExpandedWorkspaceSessions((prev) => {
        const next = new Set(prev);
        if (next.has(workspaceId)) {
          next.delete(workspaceId);
        } else {
          next.add(workspaceId);
        }
        return next;
      });

      try {
        const sessions =
          await commands.getWorkspaceSessions(workspaceId);
        setWorkspaceSessions((prev) => {
          const next = new Map(prev);
          next.set(workspaceId, sessions);
          return next;
        });
      } catch {
        // Silently fail — show empty list
      }
    },
    [],
  );

  const handleResumeSession = useCallback(
    async (workspaceId: string, sessionId: string) => {
      setOpeningSession((prev) => new Set([...prev, workspaceId]));
      try {
        await commands.resumeClaudeSession(workspaceId, sessionId);
      } finally {
        setOpeningSession((prev) => {
          const next = new Set(prev);
          next.delete(workspaceId);
          return next;
        });
        setTimeout(pollSessions, 1000);
      }
    },
    [pollSessions],
  );

  // ---- Context value ----

  const contextValue: AppContextType = {
    repos,
    workspaces,
    activeSessions,
    otherSessionCount,
    showArchived,
    expandedRepos,
    openSettingsRepo,
    newWorktreeRepo,
    creatingWorkspace,
    archivingWorkspace,
    openingSession,
    homePath,
    cloningRepo,
    cloneError,
    setCurrentView,
    setShowArchived,
    toggleRepo,
    setOpenSettingsRepo,
    setNewWorktreeRepo,
    setCloneError,
    createRepo: handleCreateRepo,
    updateRepoSettings: handleUpdateRepoSettings,
    deleteRepoById: handleDeleteRepo,
    createNewWorkspace: handleCreateWorkspace,
    archiveWorkspaceById: handleArchiveWorkspace,
    openClaude: handleOpenClaude,
    expandedWorkspaceSessions,
    workspaceSessions,
    toggleWorkspaceSessions,
    resumeSession: handleResumeSession,
  };

  return (
    <AppContext.Provider value={contextValue}>
      {currentView === "main" ? <MainView /> : <AddRepoView />}
    </AppContext.Provider>
  );
}

export default App;
