import { useState, useEffect, useCallback, useRef } from "react";
import { homeDir } from "@tauri-apps/api/path";
import {
  commands,
  type Repo,
  type Workspace,
  type JsonValue,
  type ClaudeSessionEntry,
  type WorkspacePaneInfo,
  type ContainerMode,
} from "./bindings";
import { checkForUpdates, type Update } from "./updater";
import { AppContext } from "@/lib/context";
import type { AppContextType } from "@/lib/types";
import { TreePanel } from "@/components/tree-panel";
import { DetailPanel } from "@/components/detail-panel";
import { CreateWorktreeModal } from "@/components/modals/create-worktree";
import { SettingsModal } from "@/components/modals/settings";
import { AddRepoModal } from "@/components/modals/add-repo";
import { ConfirmArchiveModal } from "@/components/modals/confirm-archive";
import { Button } from "@/components/ui/button";

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
  const [detectedEditors, setDetectedEditors] = useState<string[]>(["iterm"]);
  const [preferredEditor, setPreferredEditor] = useState("iterm");

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
      commands.detectEditors().then(setDetectedEditors).catch(() => {});
      commands
        .getSetting("preferred_editor")
        .then((s) => setPreferredEditor(s.value))
        .catch(() => {});
      pollSessions();
      checkForUpdates().then(setAvailableUpdate);
    })();
  }, [pollSessions]);

  // ---- Polling interval + focus ----

  useEffect(() => {
    const interval = setInterval(pollSessions, 5000);
    const handleFocus = () => pollSessions();
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
    prevSelectedRef.current = null;
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

  const handleDeleteRepo = useCallback(
    async (id: string) => {
      await commands.deleteRepo(id);
      setRepos((prev) => prev.filter((r) => r.id !== id));
      setWorkspaces((prev) => prev.filter((ws) => ws.repository_id !== id));
      if (selectedWorktreeId) {
        const ws = workspaces.find((w) => w.id === selectedWorktreeId);
        if (ws?.repository_id === id) setSelectedWorktreeId(null);
      }
    },
    [selectedWorktreeId, workspaces],
  );

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

  const handleOpenInEditor = useCallback(
    async (workspaceId: string, editorId?: string) => {
      const editor = editorId ?? preferredEditor;
      setOpeningSession((prev: Set<string>) => new Set([...prev, workspaceId]));
      try {
        await commands.openInEditor(workspaceId, editor);
      } finally {
        setOpeningSession((prev: Set<string>) => {
          const next = new Set(prev);
          next.delete(workspaceId);
          return next;
        });
        setTimeout(pollSessions, 1000);
      }
    },
    [pollSessions, preferredEditor],
  );

  const handleSetPreferredEditor = useCallback(async (editorId: string) => {
    setPreferredEditor(editorId);
    try {
      await commands.setSetting("preferred_editor", editorId);
    } catch {
      // silent
    }
  }, []);

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
        const currentIndex = visibleWorkspaces.findIndex(
          (ws: Workspace) => ws.id === selectedWorktreeId,
        );
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
    openInEditor: handleOpenInEditor,
    killPane: handleKillPane,
    detectedEditors,
    preferredEditor,
    resumeSession: handleResumeSession,
  };

  return (
    <AppContext.Provider value={contextValue}>
      <div className="grid grid-cols-[260px_1fr] grid-rows-[auto_1fr] h-screen overflow-hidden">
        {/* Header */}
        <header className="col-span-full flex items-center justify-between px-4 py-2 border-b bg-muted/50 h-11 drag">
          <span className="text-[15px] font-semibold text-foreground">bunyan</span>
          <div className="flex items-center gap-1.5 no-drag">
            <Button
              variant="outline"
              size="icon-sm"
              title="New worktree"
              onClick={() => setShowNewWorktree(true)}
              disabled={repos.length === 0}
            >
              +
            </Button>
            <Button
              variant="outline"
              size="icon-sm"
              title="Settings"
              onClick={() => setShowSettings(true)}
            >
              &#9881;
            </Button>
          </div>
        </header>

        {/* Update banner */}
        {availableUpdate && !updateDismissed && (
          <div className="col-span-full flex items-center gap-2 px-4 py-1.5 bg-blue-50 border-b border-blue-200 text-xs text-blue-800">
            <span>v{availableUpdate.version} available</span>
            <Button
              variant="outline"
              size="xs"
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
            </Button>
            <Button
              variant="ghost"
              size="icon-xs"
              title="Dismiss"
              onClick={() => setUpdateDismissed(true)}
            >
              &times;
            </Button>
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
          detectedEditors={detectedEditors}
          preferredEditor={preferredEditor}
          onSetPreferredEditor={handleSetPreferredEditor}
        />
      )}
      {showAddRepo && (
        <AddRepoModal
          onClose={() => {
            setShowAddRepo(false);
            setCloneError(null);
          }}
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
