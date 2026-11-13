import { Download } from "lucide-react";
import { DropdownMenu } from "radix-ui";
import { useState } from "react";
import { outlineButton } from "@/components/ui/button";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";

async function exportProcesses(format: "csv" | "json", includeCommandLines: boolean) {
  try {
    const path = await backend.exportProcesses(format, includeCommandLines);
    if (path) toast.success(`Process list saved to ${path}`);
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}

const itemCls =
  "flex cursor-default items-center gap-2 rounded px-2 py-1.5 outline-none data-[highlighted]:bg-surface-2";

/** Export the current process list. Command lines are opt-in with a warning. */
export function ExportMenu() {
  const [withCmd, setWithCmd] = useState(false);
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button
          type="button"
          disabled={!isBackendAvailable()}
          title={isBackendAvailable() ? undefined : "Export is available in the desktop app"}
          className={outlineButton}
        >
          <Download size="0.875rem" aria-hidden />
          Export
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="end"
          sideOffset={4}
          className="z-50 w-72 rounded-lg border border-border bg-surface p-1 text-[0.875rem] shadow-lg"
        >
          <DropdownMenu.Item
            className={itemCls}
            onSelect={() => void exportProcesses("csv", withCmd)}
          >
            Export as CSV…
          </DropdownMenu.Item>
          <DropdownMenu.Item
            className={itemCls}
            onSelect={() => void exportProcesses("json", withCmd)}
          >
            Export as JSON…
          </DropdownMenu.Item>
          <DropdownMenu.Separator className="my-1 h-px bg-border" />
          <DropdownMenu.CheckboxItem
            className={itemCls}
            checked={withCmd}
            onCheckedChange={(v) => setWithCmd(v === true)}
            onSelect={(e) => e.preventDefault()}
          >
            <span className="grid size-4 shrink-0 place-items-center rounded border border-border-strong">
              <DropdownMenu.ItemIndicator>
                <span className="block size-2 rounded-sm bg-accent" />
              </DropdownMenu.ItemIndicator>
            </span>
            Include command lines
          </DropdownMenu.CheckboxItem>
          {withCmd && (
            <p className="px-2 pb-1 text-[0.8125rem] text-warn">
              Command lines can contain passwords, tokens or other private data. Share the file
              carefully.
            </p>
          )}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
