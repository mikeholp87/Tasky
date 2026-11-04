import {
  ChevronsDownUp,
  ChevronsUpDown,
  Columns3,
  Layers,
  List,
  ListTree,
  Search,
  X,
} from "lucide-react";
import { DropdownMenu } from "radix-ui";
import { useEffect, useRef } from "react";
import { cn } from "@/lib/cn";
import { useProcessStore } from "@/stores/processStore";
import type { ProcessRow } from "@/types";
import { COLUMNS, FILTER_PRESETS } from "./logic";
import { ExportMenu } from "./ExportMenu";
import { groupIds } from "./apps";
import { parentPids } from "./tree";

export function ProcessToolbar({
  shown,
  total,
  rows,
}: {
  shown: number;
  total: number;
  rows: readonly ProcessRow[];
}) {
  const mode = useProcessStore((s) => s.mode);
  return (
    <div className="flex flex-wrap items-center gap-2 pb-3">
      <SearchBox />
      <PresetSelect />
      <span className="num text-fg-muted" aria-live="polite">
        {mode !== "list" || shown === total
          ? `${total} processes`
          : `${shown} of ${total} processes`}
      </span>
      <div className="ml-auto flex items-center gap-1">
        <TreeButtons rows={rows} apps={mode === "apps"} />
        <ModeToggle />
        <ColumnMenu />
        <ExportMenu />
      </div>
    </div>
  );
}

function SearchBox() {
  const search = useProcessStore((s) => s.search);
  const setSearch = useProcessStore((s) => s.setSearch);
  const focusToken = useProcessStore((s) => s.focusSearchToken);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (focusToken > 0) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [focusToken]);

  return (
    <div className="relative w-72">
      <Search
        size="0.875rem"
        aria-hidden
        className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-subtle"
      />
      <input
        ref={inputRef}
        type="search"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape" && search) {
            e.stopPropagation();
            setSearch("");
          }
        }}
        placeholder="Search name, PID, command, user…"
        aria-label="Search processes"
        className="h-8 w-full rounded-md border border-border bg-surface pl-8 pr-7 outline-none placeholder:text-fg-subtle focus:border-accent [&::-webkit-search-cancel-button]:hidden"
      />
      {search && (
        <button
          type="button"
          aria-label="Clear search"
          onClick={() => setSearch("")}
          className="absolute right-1.5 top-1/2 grid size-5 -translate-y-1/2 place-items-center rounded text-fg-subtle hover:text-fg"
        >
          <X size="0.8125rem" />
        </button>
      )}
    </div>
  );
}

function PresetSelect() {
  const preset = useProcessStore((s) => s.preset);
  const setPreset = useProcessStore((s) => s.setPreset);
  return (
    <label className="flex items-center gap-2 text-fg-muted">
      <span className="sr-only">Filter</span>
      <select
        value={preset}
        onChange={(e) => setPreset(e.target.value as typeof preset)}
        className="h-8 rounded-md border border-border bg-surface px-2 text-fg outline-none focus:border-accent"
      >
        {FILTER_PRESETS.map((p) => (
          <option key={p.id} value={p.id} title={p.description}>
            {p.id === "all" ? "All processes" : p.label}
          </option>
        ))}
      </select>
    </label>
  );
}

function ModeToggle() {
  const mode = useProcessStore((s) => s.mode);
  const setMode = useProcessStore((s) => s.setMode);
  const item = (id: typeof mode, label: string, Icon: typeof List) => (
    <button
      type="button"
      role="radio"
      aria-checked={mode === id}
      aria-label={label}
      title={label}
      onClick={() => setMode(id)}
      className={cn(
        "grid size-7 place-items-center rounded text-fg-muted",
        mode === id && "bg-surface text-fg shadow-sm",
      )}
    >
      <Icon size="0.9375rem" />
    </button>
  );
  return (
    <div role="radiogroup" aria-label="View" className="flex gap-0.5 rounded-md bg-surface-2 p-0.5">
      {item("list", "List view", List)}
      {item("tree", "Tree view", ListTree)}
      {item("apps", "Applications view (group processes by program)", Layers)}
    </div>
  );
}

function ColumnMenu() {
  const visible = useProcessStore((s) => s.visibleColumns);
  const toggle = useProcessStore((s) => s.toggleColumn);
  const reset = useProcessStore((s) => s.resetColumns);
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button
          type="button"
          className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg"
        >
          <Columns3 size="0.875rem" aria-hidden />
          Columns
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="end"
          sideOffset={4}
          className="z-50 max-h-[70vh] min-w-48 overflow-y-auto rounded-lg border border-border bg-surface p-1 text-[0.875rem] shadow-lg"
        >
          {COLUMNS.map((c) => (
            <DropdownMenu.CheckboxItem
              key={c.id}
              checked={visible.includes(c.id)}
              disabled={c.id === "name"}
              onCheckedChange={() => toggle(c.id)}
              onSelect={(e) => e.preventDefault()}
              className="flex cursor-default items-center gap-2 rounded px-2 py-1.5 outline-none data-[disabled]:opacity-50 data-[highlighted]:bg-surface-2"
            >
              <span className="grid size-4 place-items-center rounded border border-border-strong">
                <DropdownMenu.ItemIndicator>
                  <span className="block size-2 rounded-sm bg-accent" />
                </DropdownMenu.ItemIndicator>
              </span>
              {c.label}
            </DropdownMenu.CheckboxItem>
          ))}
          <DropdownMenu.Separator className="my-1 h-px bg-border" />
          <DropdownMenu.Item
            onSelect={reset}
            className="cursor-default rounded px-2 py-1.5 outline-none data-[highlighted]:bg-surface-2"
          >
            Reset columns
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

function TreeButtons({ rows, apps }: { rows: readonly ProcessRow[]; apps: boolean }) {
  const treeExpand = useProcessStore((s) => s.expandAll);
  const treeCollapse = useProcessStore((s) => s.collapseAll);
  const setExpandedApps = useProcessStore((s) => s.setExpandedApps);
  const expandAll = apps ? () => setExpandedApps(groupIds(rows)) : treeExpand;
  const collapseAll = (pids: Iterable<number>) => (apps ? setExpandedApps([]) : treeCollapse(pids));
  const cls =
    "flex h-8 items-center gap-1.5 rounded-md px-2 text-fg-muted hover:bg-surface-2 hover:text-fg";
  return (
    <>
      <button type="button" className={cls} onClick={expandAll}>
        <ChevronsUpDown size="0.875rem" aria-hidden />
        Expand all
      </button>
      <button type="button" className={cls} onClick={() => collapseAll(parentPids(rows))}>
        <ChevronsDownUp size="0.875rem" aria-hidden />
        Collapse all
      </button>
    </>
  );
}
