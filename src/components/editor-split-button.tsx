import { useContext } from "react";
import { AppContext } from "@/lib/context";
import { EDITOR_DISPLAY_NAMES } from "@/lib/helpers";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

export function EditorSplitButton({
  workspaceId,
  disabled,
}: {
  workspaceId: string;
  disabled: boolean;
}) {
  const ctx = useContext(AppContext);
  const editorName = EDITOR_DISPLAY_NAMES[ctx.preferredEditor] ?? ctx.preferredEditor;
  const otherEditors = ctx.detectedEditors.filter((e) => e !== ctx.preferredEditor);

  return (
    <div className="inline-flex">
      <Button
        variant="outline"
        size="sm"
        onClick={() => ctx.openInEditor(workspaceId)}
        disabled={disabled}
        className={otherEditors.length > 0 ? "rounded-r-none" : ""}
      >
        Open in {editorName}
      </Button>
      {otherEditors.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="outline"
              size="sm"
              disabled={disabled}
              className="rounded-l-none border-l-0 px-1.5"
            >
              &#9662;
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {otherEditors.map((editorId) => (
              <DropdownMenuItem
                key={editorId}
                onClick={() => ctx.openInEditor(workspaceId, editorId)}
              >
                {EDITOR_DISPLAY_NAMES[editorId] ?? editorId}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
