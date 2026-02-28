import { useState } from "react";
import { deriveRepoName } from "@/lib/helpers";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

export function AddRepoModal({
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
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add Repository</DialogTitle>
        </DialogHeader>
        {cloningRepo ? (
          <div className="flex items-center gap-2.5 py-3 text-sm text-muted-foreground">
            <span className="size-3.5 border-2 border-muted border-t-primary rounded-full animate-spin inline-block shrink-0" />
            <span>Cloning {derivedName}&#8230;</span>
          </div>
        ) : (
          <div className="grid gap-4">
            <div className="grid gap-2">
              <Label>Remote URL</Label>
              <Input
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="git@github.com:org/repo.git"
                autoFocus
              />
            </div>
            <div className="grid gap-2">
              <Label>Repository name (derived)</Label>
              <Input value={derivedName} readOnly className="bg-muted" />
            </div>
          </div>
        )}
        {cloneError && (
          <Alert variant="destructive">
            <AlertDescription>{cloneError}</AlertDescription>
          </Alert>
        )}
        {!cloningRepo && (
          <DialogFooter>
            <Button variant="outline" size="sm" onClick={onClose}>Cancel</Button>
            <Button
              size="sm"
              onClick={() => onClone(derivedName, url)}
              disabled={!url.trim() || !derivedName}
            >
              Clone
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
