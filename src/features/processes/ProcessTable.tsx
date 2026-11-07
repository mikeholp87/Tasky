import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, ChevronDown, ChevronRight } from "lucide-react";
import {
  type KeyboardEvent,
  type PointerEvent,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
} from "react";
import { cn } from "@/lib/cn";
import { scaled, useRootPx } from "@/hooks/useRootPx";
import { columnWidth, useProcessStore } from "@/stores/processStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useUiStore } from "@/stores/uiStore";
import type { ProcessRow } from "@/types";
import { type HeatLevel, heatLevel, memoryHeatLevel } from "@/utils/levels";
import { GROUP_MEMORY_NOTE } from "./apps";
import { cellContent, cellTitle } from "./cells";
import {
  COLUMN_BY_ID,
  type ColumnId,
  type ProcessContext,
  type SortState,
  isNewProcess,
  memoryOf,
  runState,
  type RunState,
} from "./logic";
import type { TableItem } from "./tree";

/** Execution state: a coloured row edge, plus a status-cell tint only for problem states. */
const RUN_COLOR: Record<RunState, { bar: string; bg?: string; fg?: string }> = {
  active: { bar: "var(--ok)" },
  idle: { bar: "transparent" },
  problem: { bar: "var(--crit)", bg: "var(--crit-soft)", fg: "var(--crit)" },
};

const HEAT_BG: Record<HeatLevel, string | undefined> = {
  0: undefined,
  1: "var(--heat-1)",
  2: "var(--heat-2)",
  3: "var(--heat-3)",
};

/** Indentation per tree level, in pixels. */
const INDENT = 16;

interface ProcessTableProps {
  items: readonly TableItem[];
  ctx: ProcessContext;
  selfPid: number | undefined;
  /** Renders tree affordances (indentation, expand/collapse). */
  tree?: boolean;
  /**
   * Wraps the whole table once, e.g. to attach a single context menu.
   * `find` resolves the process shown in a row, by PID.
   */
  wrapTable?: (
    table: React.ReactElement,
    find: (pid: number) => ProcessRow | undefined,
  ) => React.ReactElement;
  /** Delete key on the selected row; must ask for confirmation. */
  onRequestTerminate?: (row: ProcessRow) => void;
}

/**
 * Virtualized process table: only visible rows are rendered, so thousands
 * of processes stay cheap. Rows are keyed by PID.
 */
export function ProcessTable({
  items,
  ctx,
  selfPid,
  tree = false,
  wrapTable,
  onRequestTerminate,
}: ProcessTableProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const visibleColumns = useProcessStore((s) => s.visibleColumns);
  const widths = useProcessStore((s) => s.columnWidths);
  const sort = useProcessStore((s) => s.sort);
  const selectedPid = useProcessStore((s) => s.selectedPid);
  const select = useProcessStore((s) => s.select);
  const toggleCollapsed = useProcessStore((s) => s.toggleCollapsed);
  const setCollapsed = useProcessStore((s) => s.setCollapsed);
  const openProcess = useUiStore((s) => s.openProcess);
  const compact = useSettingsStore((s) => s.compactTable);
  const heat = useSettingsStore((s) => s.heatMapping);
  const highlightSecs = useSettingsStore((s) => s.newProcessSeconds);
  const rootPx = useRootPx();
  // Rows scale with the type size, but stop at 18px root so a big window
  // keeps showing many processes instead of fewer, taller rows.
  const rowHeight = scaled(compact ? 26 : 32, Math.min(rootPx, 18));

  const template = useMemo(
    () => visibleColumns.map((c) => `${scaled(columnWidth(widths, c), rootPx)}px`).join(" "),
    [visibleColumns, widths, rootPx],
  );
  const totalWidth = useMemo(
    () => visibleColumns.reduce((sum, c) => sum + scaled(columnWidth(widths, c), rootPx), 0),
    [visibleColumns, widths, rootPx],
  );

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
    getItemKey: (index) => items[index]?.row.pid ?? index,
    // Render a first screenful before layout is measured.
    initialRect: { width: 1200, height: 800 },
  });

  useEffect(() => {
    virtualizer.measure();
  }, [rowHeight, virtualizer]);

  const selectedIndex = useMemo(
    () => (selectedPid == null ? -1 : items.findIndex((i) => i.row.pid === selectedPid)),
    [items, selectedPid],
  );

  const onKeyDown = useCallback(
    (e: KeyboardEvent<HTMLDivElement>) => {
      if (items.length === 0) return;
      const current = items[selectedIndex];
      const page = Math.max(
        1,
        Math.floor((scrollRef.current?.clientHeight ?? 300) / rowHeight) - 1,
      );
      let next: number;
      switch (e.key) {
        case "ArrowDown":
          next = selectedIndex < 0 ? 0 : Math.min(items.length - 1, selectedIndex + 1);
          break;
        case "ArrowUp":
          next = selectedIndex < 0 ? 0 : Math.max(0, selectedIndex - 1);
          break;
        case "PageDown":
          next = Math.min(items.length - 1, Math.max(0, selectedIndex) + page);
          break;
        case "PageUp":
          next = Math.max(0, selectedIndex - page);
          break;
        case "Home":
          next = 0;
          break;
        case "End":
          next = items.length - 1;
          break;
        case "ArrowRight":
        case "ArrowLeft":
          if (tree && current?.hasChildren) {
            e.preventDefault();
            setCollapsed(current.row.pid, e.key === "ArrowLeft");
          }
          return;
        case "Enter":
          if (current?.group) toggleCollapsed(current.row.pid);
          else if (current) openProcess(current.row.pid, current.row.startTime);
          e.preventDefault();
          return;
        case "Delete":
          if (current && !current.group && onRequestTerminate) {
            e.preventDefault();
            onRequestTerminate(current.row);
          }
          return;
        default:
          return;
      }
      e.preventDefault();
      const item = items[next];
      if (item) {
        select(item.row.pid);
        virtualizer.scrollToIndex(next, { align: "auto" });
      }
    },
    [
      items,
      selectedIndex,
      rowHeight,
      tree,
      select,
      setCollapsed,
      openProcess,
      toggleCollapsed,
      onRequestTerminate,
      virtualizer,
    ],
  );

  const find = useCallback(
    (pid: number) => items.find((i) => !i.group && i.row.pid === pid)?.row,
    [items],
  );

  const table = (
    <div
      ref={scrollRef}
      role={tree ? "treegrid" : "grid"}
      aria-label="Processes"
      aria-rowcount={items.length + 1}
      aria-colcount={visibleColumns.length}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="relative min-h-0 flex-1 overflow-auto rounded-lg border border-border bg-surface focus-visible:outline-2"
    >
      <div style={{ width: totalWidth, minWidth: "100%" }}>
        <HeaderRow columns={visibleColumns} template={template} sort={sort} />
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((v) => {
            const item = items[v.index];
            if (!item) return null;
            return (
              <Row
                key={item.row.pid}
                item={item}
                index={v.index}
                columns={visibleColumns}
                template={template}
                ctx={ctx}
                heat={heat}
                highlightSecs={highlightSecs}
                tree={tree}
                selected={item.row.pid === selectedPid}
                isSelf={item.row.pid === selfPid}
                offset={v.start}
                height={rowHeight}
                onSelect={select}
                onOpen={openProcess}
                onToggle={toggleCollapsed}
              />
            );
          })}
        </div>
        {items.length === 0 && (
          <div className="p-8 text-center text-fg-muted">
            No processes match the current filters.
          </div>
        )}
      </div>
    </div>
  );
  return wrapTable ? wrapTable(table, find) : table;
}

const HeaderRow = memo(function HeaderRow({
  columns,
  template,
  sort,
}: {
  columns: ColumnId[];
  template: string;
  sort: SortState;
}) {
  const sortBy = useProcessStore((s) => s.sortBy);
  return (
    <div
      role="row"
      aria-rowindex={1}
      className="sticky top-0 z-10 grid border-b border-border bg-surface-2 text-[0.8125rem] font-medium text-fg-muted"
      style={{ gridTemplateColumns: template }}
    >
      {columns.map((id) => {
        const col = COLUMN_BY_ID[id];
        const active = sort.column === id;
        return (
          <div
            key={id}
            role="columnheader"
            aria-sort={active ? (sort.direction === "asc" ? "ascending" : "descending") : "none"}
            className={cn(
              "flex min-w-0",
              id === "name" ? "sticky left-0 z-[1] bg-surface-2" : "relative",
            )}
          >
            <button
              type="button"
              onClick={() => sortBy(id)}
              className={cn(
                "flex h-8 min-w-0 flex-1 items-center gap-1 px-2 hover:text-fg",
                col.align === "right" && "justify-end",
                active && "text-fg",
              )}
            >
              <span className="truncate">{col.label}</span>
              {active &&
                (sort.direction === "asc" ? (
                  <ArrowUp size="0.75rem" aria-hidden />
                ) : (
                  <ArrowDown size="0.75rem" aria-hidden />
                ))}
            </button>
            <ResizeHandle column={id} />
          </div>
        );
      })}
    </div>
  );
});

function ResizeHandle({ column }: { column: ColumnId }) {
  const setWidth = useProcessStore((s) => s.setColumnWidth);
  const rootPx = useRootPx();
  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startWidth = columnWidth(useProcessStore.getState().columnWidths, column);
    const move = (ev: globalThis.PointerEvent) =>
      // Stored widths are in 16px-root units; the pointer moves in screen px.
      setWidth(column, startWidth + ((ev.clientX - startX) * 16) / rootPx);
    // Fires after pointerup, pointercancel (touch interrupt, alt-tab) and any
    // other way the capture ends, so the drag can never get stuck on.
    const end = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("lostpointercapture", end);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("lostpointercapture", end);
  };
  return (
    <div
      aria-hidden
      onPointerDown={onPointerDown}
      className="absolute right-0 top-1 bottom-1 w-1.5 cursor-col-resize border-r border-border hover:border-accent"
    />
  );
}

interface RowProps {
  item: TableItem;
  index: number;
  columns: ColumnId[];
  template: string;
  ctx: ProcessContext;
  heat: boolean;
  /** Seconds a new process is highlighted; 0 is off. */
  highlightSecs: number;
  tree: boolean;
  selected: boolean;
  isSelf: boolean;
  offset: number;
  height: number;
  onSelect: (pid: number) => void;
  onOpen: (pid: number, startTime: number) => void;
  onToggle: (pid: number) => void;
}

const Row = memo(function Row({
  item,
  index,
  columns,
  template,
  ctx,
  heat,
  highlightSecs,
  tree,
  selected,
  isSelf,
  offset,
  height,
  onSelect,
  onOpen,
  onToggle,
}: RowProps) {
  const { row } = item;
  const memPct = ctx.totalMemoryBytes > 0 ? (memoryOf(row) / ctx.totalMemoryBytes) * 100 : 0;
  const isNew = isNewProcess(row.startTime, ctx.nowSecs, highlightSecs);
  const run = RUN_COLOR[runState(row)];
  return (
    <div
      role="row"
      aria-rowindex={index + 2}
      aria-selected={selected}
      aria-level={tree ? (item.depth ?? 0) + 1 : undefined}
      aria-expanded={tree && item.hasChildren ? item.expanded : undefined}
      data-pid={row.pid}
      data-group={item.group ? "" : undefined}
      data-run={heat ? runState(row) : undefined}
      onClick={() => onSelect(row.pid)}
      onDoubleClick={() => (item.group ? onToggle(row.pid) : onOpen(row.pid, row.startTime))}
      onContextMenu={() => onSelect(row.pid)}
      className={cn(
        "group absolute left-0 top-0 grid w-full items-center border-b border-border/60 text-[0.875rem]",
        selected ? "bg-accent-soft" : isNew ? "bg-accent-soft/40" : "hover:bg-surface-2",
        item.group && "font-medium",
      )}
      style={{ transform: `translateY(${offset}px)`, height, gridTemplateColumns: template }}
    >
      {columns.map((id) => {
        const col = COLUMN_BY_ID[id];
        const statusTint = heat && id === "status";
        const level: HeatLevel = !heat
          ? 0
          : id === "cpu"
            ? heatLevel(row.cpuPercent)
            : id === "memory" || id === "memoryPercent"
              ? memoryHeatLevel(memPct)
              : 0;
        return (
          <div
            key={id}
            role="gridcell"
            title={
              item.group && (id === "memory" || id === "memoryPercent")
                ? GROUP_MEMORY_NOTE
                : cellTitle(id, row)
            }
            className={cn(
              "flex h-full min-w-0 items-center px-2",
              col.align === "right" && "num justify-end",
              id === "name" && "font-medium",
              // Stay in view while the other columns scroll sideways. Needs an
              // opaque background that matches the row's state.
              id === "name" &&
                cn(
                  "sticky left-0 z-[1]",
                  selected
                    ? "bg-accent-soft"
                    : isNew
                      ? "bg-[color-mix(in_srgb,var(--accent-soft)_40%,var(--surface))]"
                      : "bg-surface group-hover:bg-surface-2",
                ),
              // Zero activity recedes so real disk I/O stands out.
              ((id === "diskRead" && row.diskReadPerSec <= 0) ||
                (id === "diskWrite" && row.diskWritePerSec <= 0)) &&
                !row.restricted &&
                "text-fg-subtle",
              (id === "exe" || id === "cmd") &&
                "selectable font-mono text-[0.8125rem] text-fg-muted",
            )}
            style={{
              background: statusTint ? (run.bg ?? HEAT_BG[level]) : HEAT_BG[level],
              color: statusTint ? run.fg : undefined,
              // A coloured edge on the first cell marks the live state.
              boxShadow: heat && id === columns[0] ? `inset 3px 0 0 0 ${run.bar}` : undefined,
            }}
          >
            {id === "name" && tree && (
              <span
                className="flex shrink-0 items-center"
                style={{ width: (item.depth ?? 0) * INDENT + 18 }}
              >
                <span style={{ width: (item.depth ?? 0) * INDENT }} />
                {item.hasChildren && (
                  <button
                    type="button"
                    tabIndex={-1}
                    aria-label={item.expanded ? `Collapse ${row.name}` : `Expand ${row.name}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      onToggle(row.pid);
                    }}
                    className="grid size-4 place-items-center rounded text-fg-muted hover:bg-border"
                  >
                    {item.expanded ? (
                      <ChevronDown size="0.8125rem" />
                    ) : (
                      <ChevronRight size="0.8125rem" />
                    )}
                  </button>
                )}
              </span>
            )}
            <span className="truncate">
              {item.group && id === "pid" ? "—" : cellContent(id, row, ctx)}
            </span>
            {id === "name" && isNew && (
              <span className="ml-2 shrink-0 rounded bg-accent-soft px-1 text-[0.6875rem] font-normal text-accent">
                New
              </span>
            )}
            {id === "name" && isSelf && (
              <span className="ml-2 shrink-0 rounded bg-surface-2 px-1 text-[0.6875rem] font-normal text-fg-muted">
                this app
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
});
