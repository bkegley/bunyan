import { useState } from "react";
import type { Repo, JsonValue } from "@/bindings";
import { EDITOR_DISPLAY_NAMES, asConfig } from "@/lib/helpers";
import type { RepoConfig } from "@/lib/types";
import { checkForUpdates } from "@/updater";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import { Separator } from "@/components/ui/separator";
import { Card, CardContent } from "@/components/ui/card";
import { Alert, AlertDescription } from "@/components/ui/alert";
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

export function SettingsModal({
  repos,
  onClose,
  onUpdateSettings,
  onDeleteRepo,
  onAddRepo,
  dockerAvailable,
  detectedEditors,
  preferredEditor,
  onSetPreferredEditor,
}: {
  repos: Repo[];
  onClose: () => void;
  onUpdateSettings: (repoId: string, config: JsonValue | null) => Promise<void>;
  onDeleteRepo: (id: string) => Promise<void>;
  onAddRepo: () => void;
  dockerAvailable: boolean;
  detectedEditors: string[];
  preferredEditor: string;
  onSetPreferredEditor: (editorId: string) => Promise<void>;
}) {
  const [checking, setChecking] = useState(false);
  const [updateStatus, setUpdateStatus] = useState<string | null>(null);

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
        </DialogHeader>
        <div className="max-h-[60vh] overflow-y-auto space-y-4">
          {/* Editor preference */}
          <div className="grid gap-2">
            <Label>Preferred editor</Label>
            <Select value={preferredEditor} onValueChange={onSetPreferredEditor}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {detectedEditors.map((editorId) => (
                  <SelectItem key={editorId} value={editorId}>
                    {EDITOR_DISPLAY_NAMES[editorId] ?? editorId}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <Separator />

          {/* Repo settings */}
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
            <p className="text-muted-foreground text-sm py-2">No repositories yet.</p>
          )}
        </div>

        <Separator />

        {/* Update check */}
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={checking}
            onClick={async () => {
              setChecking(true);
              setUpdateStatus(null);
              const update = await checkForUpdates();
              setChecking(false);
              setUpdateStatus(update ? `v${update.version} available` : "Up to date");
            }}
          >
            {checking ? "Checking..." : "Check for updates"}
          </Button>
          {updateStatus && <span className="text-xs text-muted-foreground">{updateStatus}</span>}
        </div>

        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onAddRepo}>+ Add Repo</Button>
          <Button variant="outline" size="sm" onClick={onClose}>Close</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
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
  const [skipPermissions, setSkipPermissions] = useState(
    config?.container?.dangerously_skip_permissions ?? false,
  );
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
    if (
      !window.confirm(
        `Delete ${repo.name}? This removes the repo and all its workspaces from the database.`,
      )
    )
      return;
    await onDeleteRepo(repo.id);
  };

  return (
    <Card>
      <CardContent className="pt-4 space-y-3">
        <div className="font-semibold text-sm">{repo.name}</div>
        <div className="text-xs text-muted-foreground font-mono break-all">{repo.remote_url}</div>
        <div className="grid grid-cols-2 gap-2">
          <div className="grid gap-1.5">
            <Label className="text-xs">Setup script</Label>
            <Input
              value={setupScript}
              onChange={(e) => setSetupScript(e.target.value)}
              placeholder="e.g. mise trust && make setup"
            />
          </div>
          <div className="grid gap-1.5">
            <Label className="text-xs">Run script</Label>
            <Input
              value={runScript}
              onChange={(e) => setRunScript(e.target.value)}
              placeholder="e.g. npm run dev"
            />
          </div>
        </div>
        {dockerAvailable && (
          <div className="pt-2 border-t space-y-3">
            <label className="flex items-center gap-2 text-sm cursor-pointer select-none">
              <Checkbox
                checked={containerEnabled}
                onCheckedChange={(checked) => setContainerEnabled(checked === true)}
              />
              Enable containers
            </label>
            {containerEnabled && (
              <>
                <div className="grid gap-1.5">
                  <Label className="text-xs">Image</Label>
                  <Input
                    value={containerImage}
                    onChange={(e) => setContainerImage(e.target.value)}
                    placeholder="node:22"
                  />
                </div>
                <label className="flex items-center gap-2 text-sm cursor-pointer select-none">
                  <Checkbox
                    checked={skipPermissions}
                    onCheckedChange={(checked) => {
                      if (
                        checked &&
                        !window.confirm(
                          "WARNING: This allows Claude to execute arbitrary code, write files, and make network requests without any approval prompts. Your ~/.ssh keys and ~/.claude config are mounted into the container. Only enable this if you trust the codebase completely. Continue?",
                        )
                      ) {
                        return;
                      }
                      setSkipPermissions(checked === true);
                    }}
                  />
                  Skip permissions
                </label>
                {skipPermissions && (
                  <Alert className="text-amber-800 bg-amber-50 border-amber-200">
                    <AlertDescription>
                      Claude will run without permission checks. SSH keys and API credentials are
                      accessible inside the container.
                    </AlertDescription>
                  </Alert>
                )}
              </>
            )}
          </div>
        )}
        <div className="flex gap-2 items-center">
          <Button size="sm" onClick={handleSave} disabled={!hasChanges || saving}>
            {saving ? "Saving\u2026" : "Save"}
          </Button>
          <Button variant="destructive" size="sm" onClick={handleDelete} className="ml-auto">
            Delete
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
