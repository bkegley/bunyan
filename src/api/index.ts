import { api } from "./client";
import type { components } from "./schema";

// Re-export schema types for convenience
export type Repo = components["schemas"]["Repo"];
export type Workspace = components["schemas"]["Workspace"];
export type WorkspacePaneInfo = components["schemas"]["WorkspacePaneInfo"];
export type TmuxPane = components["schemas"]["TmuxPane"];
export type ClaudeSessionEntry = components["schemas"]["ClaudeSessionEntry"];
export type Setting = components["schemas"]["Setting"];
export type PortMapping = components["schemas"]["PortMapping"];
export type ContainerMode = components["schemas"]["ContainerMode"];
export type CreateRepoInput = components["schemas"]["CreateRepoInput"];
export type CreateWorkspaceInput = components["schemas"]["CreateWorkspaceInput"];
export type UpdateRepoInput = components["schemas"]["UpdateRepoInput"];

function unwrap<T>(result: { data?: T; error?: { error: string } }): T {
  if (result.error) throw new Error(result.error.error);
  return result.data as T;
}

// --- Repos ---

export async function listRepos(): Promise<Repo[]> {
  return unwrap(await api.GET("/repos"));
}

export async function getRepo(id: string): Promise<Repo> {
  return unwrap(await api.GET("/repos/{id}", { params: { path: { id } } }));
}

export async function createRepo(input: CreateRepoInput): Promise<Repo> {
  return unwrap(await api.POST("/repos", { body: input }));
}

export async function updateRepo(input: UpdateRepoInput): Promise<Repo> {
  return unwrap(
    await api.PUT("/repos/{id}", {
      params: { path: { id: input.id } },
      body: input,
    }),
  );
}

export async function deleteRepo(id: string): Promise<void> {
  unwrap(await api.DELETE("/repos/{id}", { params: { path: { id } } }));
}

// --- Workspaces ---

export async function listWorkspaces(repositoryId: string | null): Promise<Workspace[]> {
  return unwrap(
    await api.GET("/workspaces", {
      params: { query: repositoryId ? { repo_id: repositoryId } : {} },
    }),
  );
}

export async function getWorkspace(id: string): Promise<Workspace> {
  return unwrap(await api.GET("/workspaces/{id}", { params: { path: { id } } }));
}

export async function createWorkspace(input: CreateWorkspaceInput): Promise<Workspace> {
  return unwrap(await api.POST("/workspaces", { body: input }));
}

export async function archiveWorkspace(id: string, _force: boolean): Promise<Workspace> {
  return unwrap(
    await api.POST("/workspaces/{id}/archive", { params: { path: { id } } }),
  );
}

// --- Claude Sessions ---

export async function getActiveClaudeSessions(): Promise<WorkspacePaneInfo[]> {
  return unwrap(await api.GET("/sessions/active"));
}

export async function openClaudeSession(workspaceId: string): Promise<string> {
  const result = unwrap(
    await api.POST("/workspaces/{id}/claude", { params: { path: { id: workspaceId } } }),
  );
  return result.status;
}

export async function getWorkspaceSessions(workspaceId: string): Promise<ClaudeSessionEntry[]> {
  return unwrap(
    await api.GET("/workspaces/{id}/sessions", { params: { path: { id: workspaceId } } }),
  );
}

export async function resumeClaudeSession(workspaceId: string, sessionId: string): Promise<string> {
  const result = unwrap(
    await api.POST("/workspaces/{id}/claude/resume", {
      params: { path: { id: workspaceId } },
      body: { session_id: sessionId },
    }),
  );
  return result.status;
}

export async function listWorkspacePanes(workspaceId: string): Promise<TmuxPane[]> {
  return unwrap(
    await api.GET("/workspaces/{id}/panes", { params: { path: { id: workspaceId } } }),
  );
}

export async function openShellPane(workspaceId: string): Promise<string> {
  const result = unwrap(
    await api.POST("/workspaces/{id}/shell", { params: { path: { id: workspaceId } } }),
  );
  return result.status;
}

export async function viewWorkspace(workspaceId: string): Promise<string> {
  const result = unwrap(
    await api.POST("/workspaces/{id}/view", { params: { path: { id: workspaceId } } }),
  );
  return result.status;
}

export async function killPane(workspaceId: string, paneIndex: number): Promise<string> {
  const result = unwrap(
    await api.DELETE("/workspaces/{id}/panes/{index}", {
      params: { path: { id: workspaceId, index: paneIndex } },
    }),
  );
  return result.status;
}

// --- Editors ---

export async function detectEditors(): Promise<string[]> {
  return unwrap(await api.GET("/editors"));
}

export async function openInEditor(workspaceId: string, editorId: string): Promise<string> {
  const result = unwrap(
    await api.POST("/workspaces/{id}/editor", {
      params: { path: { id: workspaceId } },
      body: { editor_id: editorId },
    }),
  );
  return result.status;
}

// --- Docker ---

export async function checkDockerAvailable(): Promise<boolean> {
  const result = unwrap(await api.GET("/docker/status"));
  return result.available;
}

export async function getContainerStatus(workspaceId: string): Promise<string> {
  const result = unwrap(
    await api.GET("/workspaces/{id}/container/status", {
      params: { path: { id: workspaceId } },
    }),
  );
  return result.status;
}

export async function getContainerPorts(workspaceId: string): Promise<PortMapping[]> {
  return unwrap(
    await api.GET("/workspaces/{id}/container/ports", {
      params: { path: { id: workspaceId } },
    }),
  );
}

// --- Settings ---

export async function getSetting(key: string): Promise<Setting> {
  return unwrap(await api.GET("/settings/{key}", { params: { path: { key } } }));
}

export async function setSetting(key: string, value: string): Promise<Setting> {
  return unwrap(
    await api.PUT("/settings/{key}", {
      params: { path: { key } },
      body: { value },
    }),
  );
}

export async function getAllSettings(): Promise<Setting[]> {
  return unwrap(await api.GET("/settings"));
}

// --- System ---

export async function getSystemInfo(): Promise<{ home_dir: string }> {
  return unwrap(await api.GET("/system/info"));
}
