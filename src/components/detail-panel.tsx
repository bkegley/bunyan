import { useContext, useState, useEffect } from "react";
import type { TmuxPane, ClaudeSessionEntry, PortMapping } from "@/bindings";
import { commands } from "@/bindings";
import { AppContext } from "@/lib/context";
import { isShellPane, relativeTime } from "@/lib/helpers";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EditorSplitButton } from "./editor-split-button";

export function DetailPanel() {
  const ctx = useContext(AppContext);

  if (!ctx.selectedWorktreeId) {
    return (
      <div className="flex flex-col overflow-y-auto">
        <div className="flex flex-col items-center justify-center h-full text-muted-foreground gap-2">
          <div className="text-sm">Select a worktree to view details</div>
        </div>
      </div>
    );
  }

  const workspace = ctx.workspaces.find((ws) => ws.id === ctx.selectedWorktreeId);
  if (!workspace) {
    return (
      <div className="flex flex-col overflow-y-auto">
        <div className="flex flex-col items-center justify-center h-full text-muted-foreground gap-2">
          <div className="text-sm">Worktree not found</div>
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
    <div className="flex flex-col overflow-y-auto">
      {/* Header */}
      <div className="px-6 pt-5 pb-4 border-b">
        <div className="text-base font-semibold text-foreground mb-1 flex items-center gap-2">
          {repo ? `${repo.name} / ` : ""}{workspace.directory_name}
          {isContainer && (
            <Badge variant="outline" className="text-indigo-600 bg-indigo-50 border-indigo-200">
              container
            </Badge>
          )}
        </div>
        <div className="text-xs text-muted-foreground font-mono">{workspace.branch}</div>
      </div>

      {/* Running panes */}
      {panes.length > 0 && (
        <div className="px-6 py-4 border-b">
          <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-2">
            Running
          </div>
          {panes.map((pane) => (
            <DetailPaneRow key={pane.pane_index} pane={pane} workspaceId={workspace.id} />
          ))}
        </div>
      )}

      {/* Ports */}
      {isContainer && !isArchived && <PortsSection workspaceId={workspace.id} />}

      {/* Sessions */}
      <div className="px-6 py-4 border-b last:border-b-0">
        <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-2">
          Sessions
        </div>
        {ctx.loadingSessions ? (
          <div className="py-2 text-xs text-muted-foreground">
            <span className="size-2.5 border-[1.5px] border-muted border-t-primary rounded-full animate-spin inline-block" />
          </div>
        ) : ctx.selectedSessions.length === 0 ? (
          <div className="py-2 text-xs text-muted-foreground">No sessions yet</div>
        ) : (
          ctx.selectedSessions
            .filter((s) => (s.message_count ?? 0) > 0)
            .slice(0, 10)
            .map((s) => (
              <DetailSessionRow key={s.session_id} session={s} workspaceId={workspace.id} />
            ))
        )}
      </div>

      {/* Actions */}
      {!isArchived && (
        <div className="px-6 py-4 flex gap-2">
          <Button size="sm" onClick={() => ctx.openClaude(workspace.id)} disabled={isOpening}>
            {isOpening && (
              <span className="size-2.5 border-[1.5px] border-primary-foreground/30 border-t-primary-foreground rounded-full animate-spin inline-block" />
            )}
            Open Claude
          </Button>
          <Button variant="outline" size="sm" onClick={() => ctx.openShell(workspace.id)} disabled={isOpening}>
            Open Shell
          </Button>
          {panes.length > 0 && (
            <Button variant="outline" size="sm" onClick={() => ctx.viewWorkspace(workspace.id)} disabled={isOpening}>
              View iTerm
            </Button>
          )}
          <EditorSplitButton workspaceId={workspace.id} disabled={isOpening} />
          <Button
            variant="destructive"
            size="sm"
            onClick={() => ctx.confirmArchive(workspace)}
            className="ml-auto"
          >
            Archive
          </Button>
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
    commands
      .getContainerPorts(workspaceId)
      .then((result) => { if (!cancelled) { setPorts(result); setLoaded(true); } })
      .catch(() => { if (!cancelled) { setPorts([]); setLoaded(true); } });
    return () => { cancelled = true; };
  }, [workspaceId]);

  if (!loaded) return null;

  return (
    <div className="px-6 py-4 border-b">
      <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-2">
        Ports
      </div>
      {ports.length === 0 ? (
        <div className="py-2 text-xs text-muted-foreground">No ports forwarded</div>
      ) : (
        ports.map((p, i) => (
          <div key={i} className="flex items-center py-1 px-2 rounded gap-2 text-xs font-mono transition-colors hover:bg-muted">
            <span>{p.host_ip}:{p.host_port}</span>
            <span className="text-muted-foreground">{"\u2192"}</span>
            <span>{p.container_port}</span>
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
    <div className="flex items-center py-1.5 px-2 rounded gap-2 text-sm transition-colors hover:bg-muted">
      <Badge
        variant="outline"
        className={
          shell
            ? "text-indigo-600 bg-indigo-50 border-indigo-200"
            : "text-amber-600 bg-amber-50 border-amber-200"
        }
      >
        {shell ? "shell" : "claude"}
      </Badge>
      <span className="flex-1 text-xs text-muted-foreground overflow-hidden text-ellipsis whitespace-nowrap">
        pid {pane.pane_pid}
        {pane.is_active && " \u00b7 active"}
      </span>
      <div className="flex gap-1 shrink-0">
        <Button
          variant="ghost"
          size="icon-xs"
          title="View in iTerm"
          onClick={() => ctx.viewWorkspace(workspaceId)}
        >
          &#8599;
        </Button>
        <Button
          variant="ghost"
          size="icon-xs"
          className="hover:text-destructive hover:bg-destructive/10"
          title="Kill pane"
          onClick={() => ctx.killPane(workspaceId, pane.pane_index)}
        >
          &times;
        </Button>
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
      className="flex items-center py-1.5 px-2 rounded cursor-pointer gap-2.5 transition-colors hover:bg-blue-50"
      onClick={() => ctx.resumeSession(workspaceId, session.session_id)}
      title="Resume this session"
    >
      <span className="flex-1 text-sm text-foreground overflow-hidden text-ellipsis whitespace-nowrap">
        {snippet}
      </span>
      <span className="text-[11px] text-muted-foreground whitespace-nowrap shrink-0 tabular-nums">
        {session.message_count ?? 0} msgs
        {session.modified ? ` \u00b7 ${relativeTime(session.modified)}` : ""}
      </span>
    </div>
  );
}
