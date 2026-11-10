import { ArrowDown, ArrowUp, ChevronDown, ChevronRight } from "lucide-react";
import { type KeyboardEvent, type PointerEvent, memo } from "react";
import { cn } from "@/lib/cn";
import { useRootPx } from "@/hooks/useRootPx";
import { columnWidth, useProcessStore } from "@/stores/processStore";
import type { ProcessRow as ProcessRowData } from "@/types";
import { type HeatLevel, heatLevel, memoryHeatLevel } from "@/utils/levels";
import { GROUP_MEMORY_NOTE } from "./apps";
import { cellContent, cellTitle } from "./cells";
import {
  COLUMN_BY_ID,
  type ColumnId,
  type SortState,
  memoryOf,
  rowDomId,
  runState,
  type RunState,
} from "./logic";

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

/** Keyboard resize step, in 16px-root units. */
const RESIZE_STEP = 8;

export const HeaderRow = memo(function HeaderRow({
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
            // Otherwise the resize handle's label joins the header's name.
            aria-label={col.label}
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
  const width = useProcessStore((s) => columnWidth(s.columnWidths, column));
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
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    // The grid also handles arrows (row expand/collapse).
    e.preventDefault();
    e.stopPropagation();
    setWidth(column, width + (e.key === "ArrowRight" ? RESIZE_STEP : -RESIZE_STEP));
  };
  const label = COLUMN_BY_ID[column].label;
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={`Resize ${label} column`}
      aria-valuenow={width}
      aria-valuemin={COLUMN_BY_ID[column].minWidth}
      aria-valuetext={`${width} pixels`}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
      className="absolute right-0 top-1 bottom-1 w-1.5 cursor-col-resize border-r border-border hover:border-accent focus-visible:border-accent"
    />
  );
}

interface RowProps {
  row: ProcessRowData;
  index: number;
  depth: number;
  hasChildren: boolean;
  expanded: boolean;
  group: boolean;
  columns: ColumnId[];
  template: string;
  /** Unix seconds; 0 unless a column shows it, so rows don't re-render on the clock. */
  nowSecs: number;
  totalMemoryBytes: number;
  heat: boolean;
  isNew: boolean;
  tree: boolean;
  selected: boolean;
  isSelf: boolean;
  offset: number;
  height: number;
  onSelect: (pid: number) => void;
  onOpen: (pid: number, startTime: number) => void;
  onToggle: (pid: number) => void;
}

function sameRow(a: ProcessRowData, b: ProcessRowData): boolean {
  if (a === b) return true;
  for (const key of Object.keys(b) as (keyof ProcessRowData)[]) {
    if (a[key] !== b[key]) return false;
  }
  return true;
}

/** Every tick delivers new row objects; compare their values instead. */
function sameProps(prev: RowProps, next: RowProps): boolean {
  for (const key of Object.keys(next) as (keyof RowProps)[]) {
    if (key !== "row" && prev[key] !== next[key]) return false;
  }
  return sameRow(prev.row, next.row);
}

export const Row = memo(function Row({
  row,
  index,
  depth,
  hasChildren,
  expanded,
  group,
  columns,
  template,
  nowSecs,
  totalMemoryBytes,
  heat,
  isNew,
  tree,
  selected,
  isSelf,
  offset,
  height,
  onSelect,
  onOpen,
  onToggle,
}: RowProps) {
  const ctx = { nowSecs, totalMemoryBytes };
  const memPct = totalMemoryBytes > 0 ? (memoryOf(row) / totalMemoryBytes) * 100 : 0;
  const run = RUN_COLOR[runState(row)];
  return (
    <div
      id={rowDomId(row.pid)}
      role="row"
      aria-rowindex={index + 2}
      aria-selected={selected}
      aria-level={tree ? depth + 1 : undefined}
      aria-expanded={tree && hasChildren ? expanded : undefined}
      data-pid={row.pid}
      data-group={group ? "" : undefined}
      data-run={heat ? runState(row) : undefined}
      onClick={() => onSelect(row.pid)}
      onDoubleClick={() => (group ? onToggle(row.pid) : onOpen(row.pid, row.startTime))}
      onContextMenu={() => onSelect(row.pid)}
      className={cn(
        "group absolute left-0 top-0 grid w-full items-center border-b border-border/60 text-[0.875rem]",
        selected ? "bg-accent-soft" : isNew ? "bg-accent-soft/40" : "hover:bg-surface-2",
        group && "font-medium",
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
              group && (id === "memory" || id === "memoryPercent")
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
              <span className="flex shrink-0 items-center" style={{ width: depth * INDENT + 18 }}>
                <span style={{ width: depth * INDENT }} />
                {hasChildren && (
                  <button
                    type="button"
                    tabIndex={-1}
                    aria-label={expanded ? `Collapse ${row.name}` : `Expand ${row.name}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      onToggle(row.pid);
                    }}
                    className="grid size-4 place-items-center rounded text-fg-muted hover:bg-border"
                  >
                    {expanded ? (
                      <ChevronDown size="0.8125rem" />
                    ) : (
                      <ChevronRight size="0.8125rem" />
                    )}
                  </button>
                )}
              </span>
            )}
            <span className="truncate">
              {group && id === "pid" ? "—" : cellContent(id, row, ctx)}
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
}, sameProps);
