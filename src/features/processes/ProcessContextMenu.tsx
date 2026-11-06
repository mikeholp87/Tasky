import {
  ChevronRight,
  Copy,
  FileSearch,
  FolderOpen,
  Gauge,
  OctagonX,
  Pause,
  Play,
  Square,
} from "lucide-react";
import { ContextMenu } from "radix-ui";
import { type ReactElement, type ReactNode, useState } from "react";
import { cn } from "@/lib/cn";
import { useUiStore } from "@/stores/uiStore";
import type { PriorityLevel, ProcessRow } from "@/types";
import { PRIORITY_LABELS, copyText, requestAction, revealExecutable } from "./actions";

const PRIORITIES: PriorityLevel[] = ["idle", "belowNormal", "normal", "aboveNormal", "high"];

const itemCls =
  "flex cursor-default select-none items-center gap-2 rounded px-2 py-1.5 outline-none data-[disabled]:opacity-40 data-[highlighted]:bg-surface-2";
const contentCls =
  "z-50 min-w-56 rounded-lg border border-border bg-surface p-1 text-[0.875rem] shadow-lg";

function Item({
  icon,
  children,
  onSelect,
  disabled,
  danger,
}: {
  icon?: ReactNode;
  children: ReactNode;
  onSelect: () => void;
  disabled?: boolean;
  danger?: boolean;
}) {
  return (
    <ContextMenu.Item
      className={cn(itemCls, danger && "text-crit")}
      onSelect={onSelect}
      disabled={disabled}
    >
      <span className="grid w-4 place-items-center" aria-hidden>
        {icon}
      </span>
      {children}
    </ContextMenu.Item>
  );
}

const Separator = () => <ContextMenu.Separator className="my-1 h-px bg-border" />;

/**
 * One context menu for the whole table. Creating a menu per row made every
 * tick re-render dozens of Radix menus; this resolves the right-clicked row
 * (by its `data-pid`) when the menu opens instead.
 */
export function ProcessContextMenu({
  find,
  children,
}: {
  find: (pid: number) => ProcessRow | undefined;
  children: ReactElement;
}) {
  const openProcess = useUiStore((s) => s.openProcess);
  const [target, setTarget] = useState<{ pid: number; startTime: number } | null>(null);
  const current = target == null ? undefined : find(target.pid);
  const row = current?.startTime === target?.startTime ? current : undefined;
  const suspended = row?.status === "stopped";
  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger
        asChild
        onContextMenu={(e) => {
          const cell = (e.target as HTMLElement).closest<HTMLElement>("[data-pid]");
          // Not on a process row (header, summary row): no menu.
          if (!cell || cell.dataset.group !== undefined) {
            setTarget(null);
            e.preventDefault();
            return;
          }
          const process = find(Number(cell.dataset.pid));
          setTarget(process ? { pid: process.pid, startTime: process.startTime } : null);
        }}
      >
        {children}
      </ContextMenu.Trigger>
      {row && (
        <ContextMenu.Portal>
          <ContextMenu.Content className={contentCls} aria-label={`Actions for ${row.name}`}>
            <ContextMenu.Label className="truncate px-2 py-1 text-[0.8125rem] text-fg-subtle">
              {row.name} · PID {row.pid}
            </ContextMenu.Label>
            <Item icon={<FileSearch size="0.875rem" />} onSelect={() => openProcess(row.pid)}>
              Open details
            </Item>
            <Item
              icon={<FolderOpen size="0.875rem" />}
              onSelect={() => void revealExecutable(row)}
              disabled={!row.exe}
            >
              Open file location
            </Item>
            <Separator />
            <Item
              icon={<Copy size="0.875rem" />}
              onSelect={() => void copyText("PID", String(row.pid))}
            >
              Copy PID
            </Item>
            <Item icon={<span />} onSelect={() => void copyText("Name", row.name)}>
              Copy name
            </Item>
            <Item
              icon={<span />}
              onSelect={() => void copyText("Executable path", row.exe)}
              disabled={!row.exe}
            >
              Copy executable path
            </Item>
            <Item
              icon={<span />}
              onSelect={() => void copyText("Command line", row.cmd)}
              disabled={!row.cmd}
            >
              Copy command line
            </Item>
            <Separator />
            {suspended ? (
              <Item
                icon={<Play size="0.875rem" />}
                onSelect={() => void requestAction(row, { type: "resume" })}
              >
                Resume
              </Item>
            ) : (
              <Item
                icon={<Pause size="0.875rem" />}
                onSelect={() => void requestAction(row, { type: "suspend" })}
              >
                Suspend
              </Item>
            )}
            <ContextMenu.Sub>
              <ContextMenu.SubTrigger className={itemCls}>
                <span className="grid w-4 place-items-center" aria-hidden>
                  <Gauge size="0.875rem" />
                </span>
                Set priority
                <ChevronRight size="0.875rem" className="ml-auto" aria-hidden />
              </ContextMenu.SubTrigger>
              <ContextMenu.Portal>
                <ContextMenu.SubContent className={contentCls} sideOffset={4}>
                  {PRIORITIES.map((p) => (
                    <Item
                      key={p}
                      onSelect={() => void requestAction(row, { type: "setPriority", priority: p })}
                    >
                      {PRIORITY_LABELS[p]}
                    </Item>
                  ))}
                </ContextMenu.SubContent>
              </ContextMenu.Portal>
            </ContextMenu.Sub>
            <Separator />
            <Item
              icon={<Square size="0.8125rem" />}
              onSelect={() => void requestAction(row, { type: "terminate" })}
              danger
            >
              End process…
            </Item>
            <Item
              icon={<OctagonX size="0.875rem" />}
              onSelect={() => void requestAction(row, { type: "forceKill" })}
              danger
            >
              Force end…
            </Item>
          </ContextMenu.Content>
        </ContextMenu.Portal>
      )}
    </ContextMenu.Root>
  );
}
