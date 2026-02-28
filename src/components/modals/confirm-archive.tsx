import { useState } from "react";
import type { Workspace } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

export function ConfirmArchiveModal({
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
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Archive Worktree</DialogTitle>
          <DialogDescription>
            {dirty ? (
              <span className="text-destructive">
                <strong>{workspace.directory_name}</strong> has uncommitted changes. Force archive?
                Changes will be lost.
              </span>
            ) : (
              <>
                Archive <strong>{workspace.directory_name}</strong>? This removes the worktree and
                kills running sessions.
              </>
            )}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose} disabled={archiving}>
            Cancel
          </Button>
          <Button variant="destructive" size="sm" disabled={archiving} onClick={() => handleArchive(dirty)}>
            {archiving && (
              <span className="size-2.5 border-[1.5px] border-white/30 border-t-white rounded-full animate-spin inline-block" />
            )}
            {dirty ? "Force Archive" : "Archive"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
