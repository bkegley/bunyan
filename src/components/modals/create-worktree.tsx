import { useState, useEffect } from "react";
import type { Repo, Workspace, ContainerMode } from "@/bindings";
import { asConfig } from "@/lib/helpers";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

export function CreateWorktreeModal({
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
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>New Worktree</DialogTitle>
        </DialogHeader>
        <div className="grid gap-4">
          <div className="grid gap-2">
            <Label>Repository</Label>
            <Select value={repoId} onValueChange={setRepoId}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {repos.map((r) => (
                  <SelectItem key={r.id} value={r.id}>{r.name}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="grid gap-2">
            <Label>Worktree name</Label>
            <Input
              value={name}
              onChange={(e) => { setName(e.target.value); setError(""); }}
              placeholder="e.g. feature-auth"
              autoFocus
            />
            {error && <p className="text-sm text-destructive">{error}</p>}
          </div>
          {selectedRepo && (
            <div className="grid gap-2">
              <Label>Base branch</Label>
              <Input value={selectedRepo.default_branch} readOnly className="bg-muted" />
            </div>
          )}
          {dockerAvailable && containerEnabled && (
            <label className="flex items-center gap-2 text-sm cursor-pointer select-none">
              <Checkbox
                checked={useContainer}
                onCheckedChange={(checked) => setUseContainer(checked === true)}
              />
              Run in container
            </label>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>Cancel</Button>
          <Button size="sm" onClick={handleCreate} disabled={creating}>
            {creating ? "Creating\u2026" : "Create"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
