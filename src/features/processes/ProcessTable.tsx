import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
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
import { columnWidth, useProcessStore } from "@/stores/processStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useUiStore } from "@/stores/uiStore";
import type { ProcessRow } from "@/types";
import { type HeatLevel, heatLevel, memoryHeatLevel } from "@/utils/levels";
import { cellContent, cellTitle } from "./cells";
import { COLUMN_BY_ID, type ColumnId, type ProcessContext, type SortState } from "./logic";

const HEAT_BG: Record<HeatLevel, string | undefined> = {
  0: undefined,
  1: "var(--heat-1)",
  2: "var(--heat-2)",
  3: "var(--heat-3)",
};

interface ProcessTableProps {
  rows: readonly ProcessRow[];
  ctx: ProcessContext;
  selfPid: number | undefined;
}

/**
 * Virtualized process table: only visible rows are rendered, so thousands
 * of processes stay cheap. Rows are keyed by PID.
 */
export function ProcessTable({ rows, ctx, selfPid }: ProcessTableProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const visibleColumns = useProcessStore((s) => s.visibleColumns);
  const widths = useProcessStore((s) => s.columnWidths);
  const sort = useProcessStore((s) => s.sort);
  const selectedPid = useProcessStore((s) => s.selectedPid);
  const select = useProcessStore((s) => s.select);
  const openProcess = useUiStore((s) => s.openProcess);
  const compact = useSettingsStore((s) => s.compactTable);
  const heat = useSettingsStore((s) => s.heatMapping);
  const rowHeight = compact ? 26 : 32;

  const template = useMemo(
    () => visibleColumns.map((c) => `${columnWidth(widths, c)}px`).join(" "),
    [visibleColumns, widths],
  );
  const totalWidth = useMemo(
    () => visibleColumns.reduce((sum, c) => sum + columnWidth(widths, c), 0),
    [visibleColumns, widths],
  );

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
    getItemKey: (index) => rows[index]?.pid ?? index,
    // Render a first screenful before layout is measured.
    initialRect: { width: 1200, height: 800 },
  });

  useEffect(() => {
    virtualizer.measure();
  }, [rowHeight, virtualizer]);

  const selectedIndex = useMemo(
    () => (selectedPid == null ? -1 : rows.findIndex((r) => r.pid === selectedPid)),
    [rows, selectedPid],
  );

  const onKeyDown = useCallback(
    (e: KeyboardEvent<HTMLDivElement>) => {
      if (rows.length === 0) return;
      const page = Math.max(
        1,
        Math.floor((scrollRef.current?.clientHeight ?? 300) / rowHeight) - 1,
      );
      let next: number;
      switch (e.key) {
        case "ArrowDown":
          next = selectedIndex < 0 ? 0 : Math.min(rows.length - 1, selectedIndex + 1);
          break;
        case "ArrowUp":
          next = selectedIndex < 0 ? 0 : Math.max(0, selectedIndex - 1);
          break;
        case "PageDown":
          next = Math.min(rows.length - 1, Math.max(0, selectedIndex) + page);
          break;
        case "PageUp":
          next = Math.max(0, selectedIndex - page);
          break;
        case "Home":
          next = 0;
          break;
        case "End":
          next = rows.length - 1;
          break;
        case "Enter":
          if (selectedPid != null) openProcess(selectedPid);
          e.preventDefault();
          return;
        default:
          return;
      }
      e.preventDefault();
      const row = rows[next];
      if (row) {
        select(row.pid);
        virtualizer.scrollToIndex(next, { align: "auto" });
      }
    },
    [rows, selectedIndex, selectedPid, rowHeight, select, openProcess, virtualizer],
  );

  return (
    <div
      ref={scrollRef}
      role="grid"
      aria-label="Processes"
      aria-rowcount={rows.length + 1}
      aria-colcount={visibleColumns.length}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="relative min-h-0 flex-1 overflow-auto rounded-lg border border-border bg-surface focus-visible:outline-2"
    >
      <div style={{ width: totalWidth, minWidth: "100%" }}>
        <HeaderRow columns={visibleColumns} template={template} sort={sort} />
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const row = rows[item.index];
            if (!row) return null;
            return (
              <Row
                key={row.pid}
                row={row}
                index={item.index}
                columns={visibleColumns}
                template={template}
                ctx={ctx}
                heat={heat}
                selected={row.pid === selectedPid}
                isSelf={row.pid === selfPid}
                offset={item.start}
                height={rowHeight}
                onSelect={select}
                onOpen={openProcess}
              />
            );
          })}
        </div>
        {rows.length === 0 && (
          <div className="p-8 text-center text-fg-muted">
            No processes match the current filters.
          </div>
        )}
      </div>
    </div>
  );
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
      className="sticky top-0 z-10 grid border-b border-border bg-surface-2 text-[12px] font-medium text-fg-muted"
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
            className="relative flex min-w-0"
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
                  <ArrowUp size={12} aria-hidden />
                ) : (
                  <ArrowDown size={12} aria-hidden />
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
  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startWidth = columnWidth(useProcessStore.getState().columnWidths, column);
    const move = (ev: globalThis.PointerEvent) =>
      setWidth(column, startWidth + ev.clientX - startX);
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
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
  row: ProcessRow;
  index: number;
  columns: ColumnId[];
  template: string;
  ctx: ProcessContext;
  heat: boolean;
  selected: boolean;
  isSelf: boolean;
  offset: number;
  height: number;
  onSelect: (pid: number) => void;
  onOpen: (pid: number) => void;
}

const Row = memo(function Row({
  row,
  index,
  columns,
  template,
  ctx,
  heat,
  selected,
  isSelf,
  offset,
  height,
  onSelect,
  onOpen,
}: RowProps) {
  const memPct = ctx.totalMemoryBytes > 0 ? (row.memoryBytes / ctx.totalMemoryBytes) * 100 : 0;
  return (
    <div
      role="row"
      aria-rowindex={index + 2}
      aria-selected={selected}
      data-pid={row.pid}
      onClick={() => onSelect(row.pid)}
      onDoubleClick={() => onOpen(row.pid)}
      className={cn(
        "absolute left-0 top-0 grid w-full items-center border-b border-border/60 text-[13px]",
        selected ? "bg-accent-soft" : "hover:bg-surface-2",
      )}
      style={{ transform: `translateY(${offset}px)`, height, gridTemplateColumns: template }}
    >
      {columns.map((id) => {
        const col = COLUMN_BY_ID[id];
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
            title={cellTitle(id, row)}
            className={cn(
              "flex h-full min-w-0 items-center px-2",
              col.align === "right" && "num justify-end",
              id === "name" && "font-medium",
              (id === "exe" || id === "cmd") && "selectable font-mono text-[12px] text-fg-muted",
            )}
            style={{ background: HEAT_BG[level] }}
          >
            <span className="truncate">{cellContent(id, row, ctx)}</span>
            {id === "name" && isSelf && (
              <span className="ml-2 shrink-0 rounded bg-surface-2 px-1 text-[10px] font-normal text-fg-muted">
                this app
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
});
