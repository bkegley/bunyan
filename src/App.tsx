import {
  useState,
  useEffect,
  useCallback,
  createContext,
  useContext,
  useRef,
} from "react";
import { homeDir } from "@tauri-apps/api/path";
import {
  commands,
  type Repo,
  type Workspace,
  type JsonValue,
  type ClaudeSessionEntry,
  type WorkspacePaneInfo,
  type TmuxPane,
  type ContainerMode,
  type PortMapping,
} from "./bindings";
import { checkForUpdates, type Update } from "./updater";
import "./App.css";

// ---------------------------------------------------------------------------
// Types & helpers
// ---------------------------------------------------------------------------

interface ContainerConfig {
  enabled: boolean;
  image?: string;
  dangerously_skip_permissions?: boolean;
}

interface RepoConfig {
  scripts?: { setup?: string; run?: string };
  runScriptMode?: string;
  container?: ContainerConfig;
}

function asConfig(val: JsonValue | null): RepoConfig | null {
  if (val && typeof val === "object" && !Array.isArray(val)) {
    return val as unknown as RepoConfig;
  }
  return null;
}

function deriveRepoName(url: string): string {
  const match = url.match(/[/:]([^/:]+?)(?:\.git)?\s*$/);
  return match ? match[1] : "";
}

const SHELLS = ["zsh", "bash", "fish", "sh"];

function isShellPane(pane: TmuxPane): boolean {
  return SHELLS.includes(pane.command);
}

function relativeTime(dateStr: string | null): string {
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

type WorktreeStatus = "active" | "shell-only" | "idle" | "archived";

function getWorktreeStatus(
  workspace: Workspace,
  paneInfo: WorkspacePaneInfo | undefined,
): WorktreeStatus {
  if (workspace.state === "archived") return "archived";
  if (!paneInfo || paneInfo.panes.length === 0) return "idle";
  const hasClaude = paneInfo.panes.some((p) => !isShellPane(p));
  if (hasClaude) return "active";
  return "shell-only";
}

function statusDotClass(status: WorktreeStatus): string {
  switch (status) {
    case "active":
      return "status-dot active";
    case "shell-only":
      return "status-dot shell-only";
    case "idle":
      return "status-dot idle";
    case "archived":
      return "status-dot archived-dot";
  }
}

function consolidateStatus(statuses: WorktreeStatus[]): WorktreeStatus {
  if (statuses.includes("active")) return "active";
  if (statuses.includes("shell-only")) return "shell-only";
  if (statuses.includes("idle")) return "idle";
  return "archived";
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

interface AppContextType {
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
  killPane: (workspaceId: string, paneIndex: number) => Promise<void>;
  resumeSession: (workspaceId: string, sessionId: string) => Promise<void>;
}

const AppContext = createContext<AppContextType>(null!);

// ---------------------------------------------------------------------------
// Tree Panel components
// ---------------------------------------------------------------------------

function TreePanel() {
  const ctx = useContext(AppContext);

  const sortedRepos = [...ctx.repos].sort((a, b) => {
    const aWs = ctx.workspaces.filter((ws) => ws.repository_id === a.id);
    const bWs = ctx.workspaces.filter((ws) => ws.repository_id === b.id);
    const aStatuses = aWs.map((ws) => getWorktreeStatus(ws, ctx.workspacePanes.get(ws.id)));
    const bStatuses = bWs.map((ws) => getWorktreeStatus(ws, ctx.workspacePanes.get(ws.id)));
    const aConsolidated = consolidateStatus(aStatuses);
    const bConsolidated = consolidateStatus(bStatuses);
    const order: Record<WorktreeStatus, number> = { active: 0, "shell-only": 1, idle: 2, archived: 3 };
    const diff = order[aConsolidated] - order[bConsolidated];
    if (diff !== 0) return diff;
    return a.name.localeCompare(b.name);
  });

  return (
    <div className="tree-panel">
      <div className="tree-scroll">
        {sortedRepos.map((repo) => (
          <RepoNode key={repo.id} repo={repo} />
        ))}
        {sortedRepos.length === 0 && (
          <div style={{ padding: "16px 12px", color: "#999", fontSize: 13 }}>
            No repos yet
          </div>
        )}
      </div>
      <div className="tree-footer">
        <label>
          <input
            type="checkbox"
            checked={ctx.showArchived}
            onChange={(e) => ctx.setShowArchived(e.target.checked)}
          />
          Show archived
        </label>
      </div>
    </div>
  );
}

function RepoNode({ repo }: { repo: Repo }) {
  const ctx = useContext(AppContext);
  const isExpanded = ctx.expandedRepos.has(repo.id);

  const repoWorkspaces = ctx.workspaces
    .filter(
      (ws) =>
        ws.repository_id === repo.id &&
        (ctx.showArchived || ws.state !== "archived"),
    )
    .sort((a, b) => {
      const aStatus = getWorktreeStatus(a, ctx.workspacePanes.get(a.id));
      const bStatus = getWorktreeStatus(b, ctx.workspacePanes.get(b.id));
      const order: Record<WorktreeStatus, number> = { active: 0, "shell-only": 1, idle: 2, archived: 3 };
      return order[aStatus] - order[bStatus];
    });

  const childStatuses = repoWorkspaces.map((ws) =>
    getWorktreeStatus(ws, ctx.workspacePanes.get(ws.id)),
  );
  const repoStatus = consolidateStatus(childStatuses);

  const mostRecent = repoWorkspaces.reduce<string | null>((best, ws) => {
    if (!best || ws.updated_at > best) return ws.updated_at;
    return best;
  }, null);

  return (
    <div className="tree-repo">
      <div className="tree-repo-row" onClick={() => ctx.toggleRepo(repo.id)}>
        <span className={`tree-chevron ${isExpanded ? "expanded" : ""}`}>&#9654;</span>
        <span className={statusDotClass(repoStatus)} />
        <span className="tree-repo-name">{repo.name}</span>
        <span className="tree-repo-time">{relativeTime(mostRecent)}</span>
      </div>
      {isExpanded &&
        repoWorkspaces.map((ws) => (
          <WorktreeNode key={ws.id} workspace={ws} repoName={repo.name} />
        ))}
    </div>
  );
}

function WorktreeNode({ workspace, repoName }: { workspace: Workspace; repoName: string }) {
  const ctx = useContext(AppContext);
  const paneInfo = ctx.workspacePanes.get(workspace.id);
  const status = getWorktreeStatus(workspace, paneInfo);
  const isSelected = ctx.selectedWorktreeId === workspace.id;
  const isArchived = workspace.state === "archived";

  return (
    <div
      className={`tree-worktree-row ${isSelected ? "selected" : ""} ${isArchived ? "archived" : ""}`}
      onClick={() => ctx.selectWorktree(workspace.id)}
      title={`${repoName} / ${workspace.directory_name}`}
    >
      <span className={statusDotClass(status)} />
      <span className="tree-worktree-name">{workspace.directory_name}</span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Detail Panel components
// ---------------------------------------------------------------------------

function DetailPanel() {
  const ctx = useContext(AppContext);

  if (!ctx.selectedWorktreeId) {
    return (
      <div className="detail-panel">
        <div className="detail-empty">
          <div className="detail-empty-text">Select a worktree to view details</div>
        </div>
      </div>
    );
  }

  const workspace = ctx.workspaces.find((ws) => ws.id === ctx.selectedWorktreeId);
  if (!workspace) {
    return (
      <div className="detail-panel">
        <div className="detail-empty">
          <div className="detail-empty-text">Worktree not found</div>
        </div>
      </div>
    );
  }

  const repo = ctx.repos.find((r) => r.id === workspace.repository_id);
  const paneInfo = ctx.workspacePanes.get(workspace.id);
  const panes = paneInfo?.panes ?? [];
  const isArchived = workspace.state === "archived";
  const isOpening = ctx.openingSession.has(workspace.id);
  const isContainer = workspace.container_mode === "container";

  return (
    <div className="detail-panel">
      <div className="detail-header">
        <div className="detail-title">
          {repo ? `${repo.name} / ` : ""}{workspace.directory_name}
          {isContainer && <span className="container-badge">container</span>}
        </div>
        <div className="detail-branch">{workspace.branch}</div>
      </div>

      {/* Running panes */}
      {panes.length > 0 && (
        <div className="detail-section">
          <div className="detail-section-title">Running</div>
          {panes.map((pane) => (
            <DetailPaneRow
              key={pane.pane_index}
              pane={pane}
              workspaceId={workspace.id}
            />
          ))}
        </div>
      )}

      {/* Ports (container workspaces only) */}
      {isContainer && !isArchived && (
        <PortsSection workspaceId={workspace.id} />
      )}

      {/* Session history */}
      <div className="detail-section">
        <div className="detail-section-title">Sessions</div>
        {ctx.loadingSessions ? (
          <div className="session-empty"><span className="spinner spinner-sm" /></div>
        ) : ctx.selectedSessions.length === 0 ? (
          <div className="session-empty">No sessions yet</div>
        ) : (
          ctx.selectedSessions
            .filter((s) => (s.message_count ?? 0) > 0)
            .slice(0, 10)
            .map((s) => (
              <DetailSessionRow
                key={s.session_id}
                session={s}
                workspaceId={workspace.id}
              />
            ))
        )}
      </div>

      {/* Actions */}
      {!isArchived && (
        <div className="detail-actions">
          <button
            className="btn btn-primary btn-sm"
            onClick={() => ctx.openClaude(workspace.id)}
            disabled={isOpening}
          >
            {isOpening ? <span className="spinner spinner-sm" /> : null}
            Open Claude
          </button>
          <button
            className="btn btn-sm"
            onClick={() => ctx.openShell(workspace.id)}
            disabled={isOpening}
          >
            Open Shell
          </button>
          {panes.length > 0 && (
            <button
              className="btn btn-sm"
              onClick={() => ctx.viewWorkspace(workspace.id)}
              disabled={isOpening}
            >
              View iTerm
            </button>
          )}
          <button
            className="btn btn-danger btn-sm"
            onClick={() => ctx.confirmArchive(workspace)}
            style={{ marginLeft: "auto" }}
          >
            Archive
          </button>
        </div>
      )}
    </div>
  );
}

function PortsSection({ workspaceId }: { workspaceId: string }) {
  const [ports, setPorts] = useState<PortMapping[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    commands.getContainerPorts(workspaceId)
      .then((result) => { if (!cancelled) { setPorts(result); setLoaded(true); } })
      .catch(() => { if (!cancelled) { setPorts([]); setLoaded(true); } });
    return () => { cancelled = true; };
  }, [workspaceId]);

  if (!loaded) return null;

  return (
    <div className="detail-section">
      <div className="detail-section-title">Ports</div>
      {ports.length === 0 ? (
        <div className="session-empty">No ports forwarded</div>
      ) : (
        ports.map((p, i) => (
          <div key={i} className="port-row">
            <span className="port-host">{p.host_ip}:{p.host_port}</span>
            <span className="port-arrow">{"\u2192"}</span>
            <span className="port-container">{p.container_port}</span>
          </div>
        ))
      )}
    </div>
  );
}

function DetailPaneRow({ pane, workspaceId }: { pane: TmuxPane; workspaceId: string }) {
  const ctx = useContext(AppContext);
  const shell = isShellPane(pane);

  return (
    <div className="detail-pane-row">
      <span className={`pane-badge ${shell ? "shell" : "claude"}`}>
        {shell ? "shell" : "claude"}
      </span>
      <span className="detail-pane-info">
        pid {pane.pane_pid}
        {pane.is_active && " \u00b7 active"}
      </span>
      <div className="detail-pane-actions">
        <button
          className="icon-btn"
          title="View in iTerm"
          onClick={() => ctx.viewWorkspace(workspaceId)}
        >
          &#8599;
        </button>
        <button
          className="icon-btn danger"
          title="Kill pane"
          onClick={() => ctx.killPane(workspaceId, pane.pane_index)}
        >
          &times;
        </button>
      </div>
    </div>
  );
}

function DetailSessionRow({
  session,
  workspaceId,
}: {
  session: ClaudeSessionEntry;
  workspaceId: string;
}) {
  const ctx = useContext(AppContext);

  const snippet = session.first_prompt
    ? session.first_prompt.length > 80
      ? session.first_prompt.slice(0, 80) + "\u2026"
      : session.first_prompt
    : "(no prompt)";

  return (
    <div
      className="detail-session-row"
      onClick={() => ctx.resumeSession(workspaceId, session.session_id)}
      title="Resume this session"
    >
      <span className="session-prompt">{snippet}</span>
      <span className="session-meta">
        {session.message_count ?? 0} msgs
        {session.modified ? ` \u00b7 ${relativeTime(session.modified)}` : ""}
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Modals
// ---------------------------------------------------------------------------

function ConfirmArchiveModal({
  workspace,
  onClose,
  onConfirm,
}: {
  workspace: Workspace;
  onClose: () => void;
  onConfirm: (id: string, force?: boolean) => Promise<void>;
}) {
  const [archiving, setArchiving] = useState(false);
  const [dirty, setDirty] = useState(false);

  const handleArchive = async (force: boolean) => {
    setArchiving(true);
    try {
      await onConfirm(workspace.id, force);
    } catch (err: unknown) {
      const msg = String(err);
      if (!force && (msg.includes("modified or untracked") || msg.includes("--force"))) {
        setDirty(true);
        setArchiving(false);
      } else {
        console.error("[archive] error:", err);
        setArchiving(false);
      }
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Archive Worktree</h2>
        {dirty ? (
          <p style={{ fontSize: 13, color: "#c00", marginBottom: 16 }}>
            <strong>{workspace.directory_name}</strong> has uncommitted changes. Force archive? Changes will be lost.
          </p>
        ) : (
          <p style={{ fontSize: 13, color: "#555", marginBottom: 16 }}>
            Archive <strong>{workspace.directory_name}</strong>? This removes the worktree and kills running sessions.
          </p>
        )}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button className="btn btn-sm" onClick={onClose} disabled={archiving}>
            Cancel
          </button>
          <button
            className="btn btn-danger btn-sm"
            disabled={archiving}
            onClick={() => handleArchive(dirty)}
          >
            {archiving ? <span className="spinner spinner-sm" /> : dirty ? "Force Archive" : "Archive"}
          </button>
        </div>
      </div>
    </div>
  );
}

function CreateWorktreeModal({
  repos,
  onClose,
  onCreate,
  workspaces,
  dockerAvailable,
}: {
  repos: Repo[];
  onClose: () => void;
  onCreate: (repoId: string, name: string, branch: string, containerMode?: ContainerMode) => Promise<void>;
  workspaces: Workspace[];
  dockerAvailable: boolean;
}) {
  const [repoId, setRepoId] = useState(repos[0]?.id ?? "");
  const [name, setName] = useState("");
  const [useContainer, setUseContainer] = useState(false);
  const [error, setError] = useState("");
  const [creating, setCreating] = useState(false);

  const selectedRepo = repos.find((r) => r.id === repoId);
  const repoConfig = asConfig(selectedRepo?.config ?? null);
  const containerEnabled = repoConfig?.container?.enabled ?? false;

  // Reset container checkbox when repo changes
  useEffect(() => {
    setUseContainer(containerEnabled);
  }, [repoId, containerEnabled]);

  const validate = (): string => {
    if (!name.trim()) return "Name is required";
    if (/\s/.test(name)) return "No spaces allowed";
    const existing = workspaces.filter((ws) => ws.repository_id === repoId);
    if (existing.some((ws) => ws.directory_name === name)) return "Name already exists";
    return "";
  };

  const handleCreate = async () => {
    const err = validate();
    if (err) { setError(err); return; }
    setCreating(true);
    try {
      await onCreate(repoId, name, name, useContainer ? "container" : "local");
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>New Worktree</h2>
        <div className="form-group">
          <label>Repository</label>
          <select value={repoId} onChange={(e) => setRepoId(e.target.value)}>
            {repos.map((r) => (
              <option key={r.id} value={r.id}>{r.name}</option>
            ))}
          </select>
        </div>
        <div className="form-group">
          <label>Worktree name</label>
          <input
            type="text"
            value={name}
            onChange={(e) => { setName(e.target.value); setError(""); }}
            placeholder="e.g. feature-auth"
            autoFocus
          />
          {error && <div className="validation-error">{error}</div>}
        </div>
        {selectedRepo && (
          <div className="form-group">
            <label>Base branch</label>
            <input type="text" value={selectedRepo.default_branch} readOnly />
          </div>
        )}
        {dockerAvailable && containerEnabled && (
          <div className="form-group">
            <label className="container-checkbox">
              <input
                type="checkbox"
                checked={useContainer}
                onChange={(e) => setUseContainer(e.target.checked)}
              />
              Run in container
            </label>
          </div>
        )}
        <div className="form-actions">
          <button className="btn btn-sm" onClick={onClose}>Cancel</button>
          <button className="btn btn-primary btn-sm" onClick={handleCreate} disabled={creating}>
            {creating ? "Creating\u2026" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}

function AddRepoModal({
  onClose,
  onClone,
  cloningRepo,
  cloneError,
}: {
  onClose: () => void;
  onClone: (name: string, url: string) => Promise<void>;
  cloningRepo: boolean;
  cloneError: string | null;
}) {
  const [url, setUrl] = useState("");
  const derivedName = deriveRepoName(url);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Add Repository</h2>
        {cloningRepo ? (
          <div className="clone-loading">
            <span className="spinner" />
            <span>Cloning {derivedName}&#8230;</span>
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
              <label>Repository name (derived)</label>
              <input type="text" value={derivedName} readOnly />
            </div>
            <div className="form-actions">
              <button className="btn btn-sm" onClick={onClose}>Cancel</button>
              <button
                className="btn btn-primary btn-sm"
                onClick={() => onClone(derivedName, url)}
                disabled={!url.trim() || !derivedName}
              >
                Clone
              </button>
            </div>
          </>
        )}
        {cloneError && <div className="error-message">{cloneError}</div>}
      </div>
    </div>
  );
}

function SettingsModal({
  repos,
  onClose,
  onUpdateSettings,
  onDeleteRepo,
  onAddRepo,
  dockerAvailable,
}: {
  repos: Repo[];
  onClose: () => void;
  onUpdateSettings: (repoId: string, config: JsonValue | null) => Promise<void>;
  onDeleteRepo: (id: string) => Promise<void>;
  onAddRepo: () => void;
  dockerAvailable: boolean;
}) {
  const [checking, setChecking] = useState(false);
  const [updateStatus, setUpdateStatus] = useState<string | null>(null);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={{ width: 520 }}>
        <h2>Settings</h2>
        <div style={{ maxHeight: "60vh", overflowY: "auto" }}>
          {repos.map((repo) => (
            <RepoSettingsItem
              key={repo.id}
              repo={repo}
              onUpdateSettings={onUpdateSettings}
              onDeleteRepo={onDeleteRepo}
              dockerAvailable={dockerAvailable}
            />
          ))}
          {repos.length === 0 && (
            <div style={{ color: "#999", fontSize: 13, padding: "8px 0" }}>
              No repositories yet.
            </div>
          )}
        </div>
        <div style={{ borderTop: "1px solid #333", marginTop: 16, paddingTop: 12 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <button
              className="btn btn-sm"
              disabled={checking}
              onClick={async () => {
                setChecking(true);
                setUpdateStatus(null);
                const update = await checkForUpdates();
                setChecking(false);
                setUpdateStatus(
                  update ? `v${update.version} available` : "Up to date"
                );
              }}
            >
              {checking ? "Checking..." : "Check for updates"}
            </button>
            {updateStatus && (
              <span style={{ fontSize: 12, color: "#999" }}>{updateStatus}</span>
            )}
          </div>
        </div>
        <div className="form-actions" style={{ marginTop: 16 }}>
          <button className="btn btn-sm" onClick={onAddRepo}>+ Add Repo</button>
          <button className="btn btn-sm" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}

function RepoSettingsItem({
  repo,
  onUpdateSettings,
  onDeleteRepo,
  dockerAvailable,
}: {
  repo: Repo;
  onUpdateSettings: (repoId: string, config: JsonValue | null) => Promise<void>;
  onDeleteRepo: (id: string) => Promise<void>;
  dockerAvailable: boolean;
}) {
  const config = asConfig(repo.config);
  const [setupScript, setSetupScript] = useState(config?.scripts?.setup ?? "");
  const [runScript, setRunScript] = useState(config?.scripts?.run ?? "");
  const [containerEnabled, setContainerEnabled] = useState(config?.container?.enabled ?? false);
  const [containerImage, setContainerImage] = useState(config?.container?.image ?? "node:22");
  const [skipPermissions, setSkipPermissions] = useState(config?.container?.dangerously_skip_permissions ?? false);
  const [saving, setSaving] = useState(false);

  const hasChanges =
    setupScript !== (config?.scripts?.setup ?? "") ||
    runScript !== (config?.scripts?.run ?? "") ||
    containerEnabled !== (config?.container?.enabled ?? false) ||
    containerImage !== (config?.container?.image ?? "node:22") ||
    skipPermissions !== (config?.container?.dangerously_skip_permissions ?? false);

  const handleSave = async () => {
    setSaving(true);
    try {
      const newConfig: RepoConfig = {
        scripts: {
          ...(setupScript ? { setup: setupScript } : {}),
          ...(runScript ? { run: runScript } : {}),
        },
        ...(config?.runScriptMode ? { runScriptMode: config.runScriptMode } : {}),
        container: {
          enabled: containerEnabled,
          image: containerImage,
          dangerously_skip_permissions: skipPermissions,
        },
      };
      await onUpdateSettings(repo.id, newConfig as unknown as JsonValue);
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!window.confirm(`Delete ${repo.name}? This removes the repo and all its workspaces from the database.`)) return;
    await onDeleteRepo(repo.id);
  };

  return (
    <div className="settings-repo-item">
      <div className="settings-repo-name">{repo.name}</div>
      <div className="settings-repo-url">{repo.remote_url}</div>
      <div className="settings-form-row">
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
      </div>
      {dockerAvailable && (
        <div className="container-settings">
          <div className="form-group">
            <label className="container-checkbox">
              <input
                type="checkbox"
                checked={containerEnabled}
                onChange={(e) => setContainerEnabled(e.target.checked)}
              />
              Enable containers
            </label>
          </div>
          {containerEnabled && (
            <>
              <div className="form-group">
                <label>Image</label>
                <input
                  type="text"
                  value={containerImage}
                  onChange={(e) => setContainerImage(e.target.value)}
                  placeholder="node:22"
                />
              </div>
              <div className="form-group">
                <label className="container-checkbox">
                  <input
                    type="checkbox"
                    checked={skipPermissions}
                    onChange={(e) => {
                      if (e.target.checked && !window.confirm(
                        "WARNING: This allows Claude to execute arbitrary code, write files, and make network requests without any approval prompts. Your ~/.ssh keys and ~/.claude config are mounted into the container. Only enable this if you trust the codebase completely. Continue?"
                      )) {
                        return;
                      }
                      setSkipPermissions(e.target.checked);
                    }}
                  />
                  Skip permissions
                </label>
                {skipPermissions && (
                  <div className="settings-warning">
                    Claude will run without permission checks. SSH keys and API credentials are accessible inside the container.
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      )}
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <button className="btn btn-sm btn-primary" onClick={handleSave} disabled={!hasChanges || saving}>
          {saving ? "Saving\u2026" : "Save"}
        </button>
        <button className="btn btn-sm btn-danger" onClick={handleDelete} style={{ marginLeft: "auto" }}>
          Delete
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// App root
// ---------------------------------------------------------------------------

function App() {
  // Data
  const [repos, setRepos] = useState<Repo[]>([]);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspacePanes, setWorkspacePanes] = useState<Map<string, WorkspacePaneInfo>>(new Map());
  const [homePath, setHomePath] = useState("");
  const [dockerAvailable, setDockerAvailable] = useState(false);

  // UI state
  const [expandedRepos, setExpandedRepos] = useState<Set<string>>(new Set());
  const [showArchived, setShowArchived] = useState(false);
  const [selectedWorktreeId, setSelectedWorktreeId] = useState<string | null>(null);

  // Modals
  const [showNewWorktree, setShowNewWorktree] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showAddRepo, setShowAddRepo] = useState(false);
  const [cloningRepo, setCloningRepo] = useState(false);
  const [cloneError, setCloneError] = useState<string | null>(null);
  const [archiveTarget, setArchiveTarget] = useState<Workspace | null>(null);

  // Updater
  const [availableUpdate, setAvailableUpdate] = useState<Update | null>(null);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  const [installing, setInstalling] = useState(false);

  // Loading
  const [openingSession, setOpeningSession] = useState<Set<string>>(new Set());

  // Sessions for selected worktree
  const [selectedSessions, setSelectedSessions] = useState<ClaudeSessionEntry[]>([]);
  const [loadingSessions, setLoadingSessions] = useState(false);
  const prevSelectedRef = useRef<string | null>(null);

  // ---- Polling ----

  const pollSessions = useCallback(async () => {
    try {
      const paneInfos = await commands.getActiveClaudeSessions();
      const paneMap = new Map<string, WorkspacePaneInfo>();
      for (const info of paneInfos) {
        paneMap.set(info.workspace_id, info);
      }
      setWorkspacePanes(paneMap);
    } catch {
      // silent
    }
  }, []);

  // ---- Initial load ----

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
      commands.checkDockerAvailable().then(setDockerAvailable).catch(() => {});
      pollSessions();
      checkForUpdates().then(setAvailableUpdate);
    })();
  }, [pollSessions]);

  // ---- Polling interval + focus ----

  useEffect(() => {
    const interval = setInterval(pollSessions, 5000);
    const handleFocus = () => pollSessions();
    const handleVisibility = () => { if (!document.hidden) pollSessions(); };
    window.addEventListener("focus", handleFocus);
    document.addEventListener("visibilitychange", handleVisibility);
    return () => {
      clearInterval(interval);
      window.removeEventListener("focus", handleFocus);
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, [pollSessions]);

  // ---- Load sessions when selection changes ----

  useEffect(() => {
    if (!selectedWorktreeId) {
      setSelectedSessions([]);
      prevSelectedRef.current = null;
      return;
    }
    if (selectedWorktreeId === prevSelectedRef.current) return;
    prevSelectedRef.current = selectedWorktreeId;
    setLoadingSessions(true);
    commands
      .getWorkspaceSessions(selectedWorktreeId)
      .then((sessions) => setSelectedSessions(sessions))
      .catch(() => setSelectedSessions([]))
      .finally(() => setLoadingSessions(false));
  }, [selectedWorktreeId]);

  // ---- Actions ----

  const toggleRepo = useCallback((id: string) => {
    setExpandedRepos((prev: Set<string>) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const selectWorktree = useCallback((id: string) => {
    setSelectedWorktreeId(id);
    prevSelectedRef.current = null; // force session reload
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
          config: null,
        });
        setRepos((prev) => [...prev, repo]);
        setExpandedRepos((prev: Set<string>) => new Set([...prev, repo.id]));
        setShowAddRepo(false);
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
        config,
      });
      setRepos((prev) => prev.map((r) => (r.id === repo.id ? repo : r)));
    },
    [],
  );

  const handleDeleteRepo = useCallback(async (id: string) => {
    await commands.deleteRepo(id);
    setRepos((prev) => prev.filter((r) => r.id !== id));
    setWorkspaces((prev) => prev.filter((ws) => ws.repository_id !== id));
    if (selectedWorktreeId) {
      const ws = workspaces.find((w) => w.id === selectedWorktreeId);
      if (ws?.repository_id === id) setSelectedWorktreeId(null);
    }
  }, [selectedWorktreeId, workspaces]);

  const handleCreateWorkspace = useCallback(
    async (repoId: string, name: string, branch: string, containerMode?: ContainerMode) => {
      const ws = await commands.createWorkspace({
        repository_id: repoId,
        directory_name: name,
        branch,
        ...(containerMode ? { container_mode: containerMode } : {}),
      });
      setWorkspaces((prev) => [...prev, ws]);
      setSelectedWorktreeId(ws.id);
      prevSelectedRef.current = null;
      setTimeout(pollSessions, 500);
    },
    [pollSessions],
  );

  const handleArchiveWorkspace = useCallback(
    async (id: string, force = false) => {
      const updated = await commands.archiveWorkspace(id, force);
      setWorkspaces((prev) => prev.map((ws) => (ws.id === updated.id ? updated : ws)));
      if (selectedWorktreeId === id) setSelectedWorktreeId(null);
      setArchiveTarget(null);
      setTimeout(pollSessions, 500);
    },
    [pollSessions, selectedWorktreeId],
  );

  const handleOpenClaude = useCallback(
    async (workspaceId: string) => {
      setOpeningSession((prev: Set<string>) => new Set([...prev, workspaceId]));
      try {
        await commands.openClaudeSession(workspaceId);
      } finally {
        setOpeningSession((prev: Set<string>) => {
          const next = new Set(prev);
          next.delete(workspaceId);
          return next;
        });
        setTimeout(pollSessions, 1000);
      }
    },
    [pollSessions],
  );

  const handleOpenShell = useCallback(
    async (workspaceId: string) => {
      setOpeningSession((prev: Set<string>) => new Set([...prev, workspaceId]));
      try {
        await commands.openShellPane(workspaceId);
      } finally {
        setOpeningSession((prev: Set<string>) => {
          const next = new Set(prev);
          next.delete(workspaceId);
          return next;
        });
        setTimeout(pollSessions, 1000);
      }
    },
    [pollSessions],
  );

  const handleViewWorkspace = useCallback(
    async (workspaceId: string) => {
      setOpeningSession((prev: Set<string>) => new Set([...prev, workspaceId]));
      try {
        await commands.viewWorkspace(workspaceId);
      } finally {
        setOpeningSession((prev: Set<string>) => {
          const next = new Set(prev);
          next.delete(workspaceId);
          return next;
        });
        setTimeout(pollSessions, 1000);
      }
    },
    [pollSessions],
  );

  const handleKillPane = useCallback(
    async (workspaceId: string, paneIndex: number) => {
      try {
        await commands.killPane(workspaceId, paneIndex);
      } finally {
        setTimeout(pollSessions, 500);
      }
    },
    [pollSessions],
  );

  const handleResumeSession = useCallback(
    async (workspaceId: string, sessionId: string) => {
      setOpeningSession((prev: Set<string>) => new Set([...prev, workspaceId]));
      try {
        await commands.resumeClaudeSession(workspaceId, sessionId);
      } finally {
        setOpeningSession((prev: Set<string>) => {
          const next = new Set(prev);
          next.delete(workspaceId);
          return next;
        });
        setTimeout(pollSessions, 1000);
      }
    },
    [pollSessions],
  );

  // ---- Keyboard navigation ----

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      const visibleWorkspaces = workspaces.filter(
        (ws: Workspace) => showArchived || ws.state !== "archived",
      );

      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        if (visibleWorkspaces.length === 0) return;
        const currentIndex = visibleWorkspaces.findIndex((ws: Workspace) => ws.id === selectedWorktreeId);
        let nextIndex: number;
        if (e.key === "ArrowDown") {
          nextIndex = currentIndex < visibleWorkspaces.length - 1 ? currentIndex + 1 : 0;
        } else {
          nextIndex = currentIndex > 0 ? currentIndex - 1 : visibleWorkspaces.length - 1;
        }
        const nextWs = visibleWorkspaces[nextIndex];
        setExpandedRepos((prev: Set<string>) => new Set([...prev, nextWs.repository_id]));
        selectWorktree(nextWs.id);
      }

      if (e.key === "Enter" && selectedWorktreeId) {
        e.preventDefault();
        if (e.shiftKey) {
          handleOpenShell(selectedWorktreeId);
        } else {
          handleOpenClaude(selectedWorktreeId);
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [workspaces, selectedWorktreeId, showArchived, selectWorktree, handleOpenClaude, handleOpenShell]);

  // ---- Context ----

  const contextValue: AppContextType = {
    repos,
    workspaces,
    workspacePanes,
    showArchived,
    expandedRepos,
    selectedWorktreeId,
    openingSession,
    homePath,
    dockerAvailable,
    selectedSessions,
    loadingSessions,
    setShowArchived,
    toggleRepo,
    selectWorktree,
    createRepo: handleCreateRepo,
    updateRepoSettings: handleUpdateRepoSettings,
    deleteRepoById: handleDeleteRepo,
    createNewWorkspace: handleCreateWorkspace,
    archiveWorkspaceById: handleArchiveWorkspace,
    confirmArchive: setArchiveTarget,
    openClaude: handleOpenClaude,
    openShell: handleOpenShell,
    viewWorkspace: handleViewWorkspace,
    killPane: handleKillPane,
    resumeSession: handleResumeSession,
  };

  return (
    <AppContext.Provider value={contextValue}>
      <div className="app">
        {/* Header */}
        <header className="app-header">
          <span className="app-header-title">bunyan</span>
          <div className="app-header-actions">
            <button
              className="btn-icon"
              title="New worktree"
              onClick={() => setShowNewWorktree(true)}
              disabled={repos.length === 0}
            >
              +
            </button>
            <button
              className="btn-icon"
              title="Settings"
              onClick={() => setShowSettings(true)}
            >
              &#9881;
            </button>
          </div>
        </header>

        {/* Update banner */}
        {availableUpdate && !updateDismissed && (
          <div className="update-banner">
            <span>v{availableUpdate.version} available</span>
            <button
              className="btn btn-sm"
              disabled={installing}
              onClick={async () => {
                setInstalling(true);
                try {
                  await availableUpdate.downloadAndInstall();
                } catch (e) {
                  console.error("Update failed:", e);
                  setInstalling(false);
                }
              }}
            >
              {installing ? "Installing..." : "Update"}
            </button>
            <button
              className="btn-icon"
              title="Dismiss"
              onClick={() => setUpdateDismissed(true)}
            >
              &times;
            </button>
          </div>
        )}

        {/* Tree + Detail */}
        <TreePanel />
        <DetailPanel />
      </div>

      {/* Modals */}
      {showNewWorktree && (
        <CreateWorktreeModal
          repos={repos}
          workspaces={workspaces}
          onClose={() => setShowNewWorktree(false)}
          onCreate={handleCreateWorkspace}
          dockerAvailable={dockerAvailable}
        />
      )}
      {showSettings && (
        <SettingsModal
          repos={repos}
          onClose={() => setShowSettings(false)}
          onUpdateSettings={handleUpdateRepoSettings}
          onDeleteRepo={handleDeleteRepo}
          onAddRepo={() => {
            setShowSettings(false);
            setShowAddRepo(true);
          }}
          dockerAvailable={dockerAvailable}
        />
      )}
      {showAddRepo && (
        <AddRepoModal
          onClose={() => { setShowAddRepo(false); setCloneError(null); }}
          onClone={handleCreateRepo}
          cloningRepo={cloningRepo}
          cloneError={cloneError}
        />
      )}
      {archiveTarget && (
        <ConfirmArchiveModal
          workspace={archiveTarget}
          onClose={() => setArchiveTarget(null)}
          onConfirm={handleArchiveWorkspace}
        />
      )}
    </AppContext.Provider>
  );
}

export default App;
