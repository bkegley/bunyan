import { cn } from "@/lib/utils";
import type { WorktreeStatus } from "@/lib/types";

const statusClasses: Record<WorktreeStatus, string> = {
  active: "bg-green-500 animate-pulse-dot",
  "shell-only": "bg-blue-500",
  idle: "bg-transparent border-[1.5px] border-muted-foreground/30",
  archived: "bg-transparent border-[1.5px] border-muted-foreground/20",
};

export function StatusDot({ status, className }: { status: WorktreeStatus; className?: string }) {
  return <span className={cn("size-2 rounded-full shrink-0", statusClasses[status], className)} />;
}
