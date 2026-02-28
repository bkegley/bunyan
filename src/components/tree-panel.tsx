import { useContext } from "react";
import type { Repo, Workspace } from "@/bindings";
import { AppContext } from "@/lib/context";
import type { WorktreeStatus } from "@/lib/types";
import { getWorktreeStatus, consolidateStatus, relativeTime } from "@/lib/helpers";
import { StatusDot } from "./status-dot";
import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/utils";

const statusOrder: Record<WorktreeStatus, number> = {
  active: 0,
  "shell-only": 1,
  idle: 2,
  archived: 3,
};

export function TreePanel() {
  const ctx = useContext(AppContext);

  const sortedRepos = [...ctx.repos].sort((a, b) => {
    const aWs = ctx.workspaces.filter((ws) => ws.repository_id === a.id);
    const bWs = ctx.workspaces.filter((ws) => ws.repository_id === b.id);
    const aStatuses = aWs.map((ws) => getWorktreeStatus(ws, ctx.workspacePanes.get(ws.id)));
    const bStatuses = bWs.map((ws) => getWorktreeStatus(ws, ctx.workspacePanes.get(ws.id)));
    const diff = statusOrder[consolidateStatus(aStatuses)] - statusOrder[consolidateStatus(bStatuses)];
    if (diff !== 0) return diff;
    return a.name.localeCompare(b.name);
  });

  return (
    <div className="flex flex-col border-r bg-muted/50 overflow-hidden">
      <div className="flex-1 overflow-y-auto py-1">
        {sortedRepos.map((repo) => (
          <RepoNode key={repo.id} repo={repo} />
        ))}
        {sortedRepos.length === 0 && (
          <div className="px-3 py-4 text-muted-foreground text-sm">No repos yet</div>
        )}
      </div>
      <div className="px-3 py-2 border-t text-xs text-muted-foreground">
        <label className="flex items-center gap-1.5 cursor-pointer select-none">
          <Checkbox
            checked={ctx.showArchived}
            onCheckedChange={(checked) => ctx.setShowArchived(checked === true)}
            className="size-3.5"
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
      return statusOrder[aStatus] - statusOrder[bStatus];
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
    <div className="select-none">
      <div
        className="flex items-center px-3 py-1.5 cursor-pointer gap-2 transition-colors hover:bg-muted"
        onClick={() => ctx.toggleRepo(repo.id)}
      >
        <span
          className={cn(
            "text-[9px] text-muted-foreground w-3 inline-flex items-center justify-center shrink-0 transition-transform duration-150",
            isExpanded && "rotate-90",
          )}
        >
          &#9654;
        </span>
        <StatusDot status={repoStatus} />
        <span className="text-[13px] font-semibold text-foreground flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
          {repo.name}
        </span>
        <span className="text-[11px] text-muted-foreground shrink-0 tabular-nums">
          {relativeTime(mostRecent)}
        </span>
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
      className={cn(
        "flex items-center py-1.5 px-3 pl-8 cursor-pointer gap-2 transition-colors border-l-[3px] border-transparent hover:bg-muted",
        isSelected && "bg-blue-50 border-l-blue-500",
        isArchived && "opacity-45",
      )}
      onClick={() => ctx.selectWorktree(workspace.id)}
      title={`${repoName} / ${workspace.directory_name}`}
    >
      <StatusDot status={status} />
      <span
        className={cn(
          "text-[13px] text-muted-foreground flex-1 overflow-hidden text-ellipsis whitespace-nowrap",
          isSelected && "text-foreground font-medium",
        )}
      >
        {workspace.directory_name}
      </span>
    </div>
  );
}
