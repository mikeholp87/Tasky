import { useVirtualizer } from "@tanstack/react-virtual";
import {
  type KeyboardEvent,
  type MouseEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
} from "react";
import { scaled, useRootPx } from "@/hooks/useRootPx";
import { columnWidth, useProcessStore } from "@/stores/processStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useUiStore } from "@/stores/uiStore";
import type { ProcessRow } from "@/types";
import { type ProcessContext, isNewProcess, rowDomId } from "./logic";
import { HeaderRow, Row } from "./ProcessRow";
import type { TableItem } from "./tree";

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
  // Only a visible runtime column makes rows depend on the clock.
  const rowNow = visibleColumns.includes("runtime") ? ctx.nowSecs : 0;

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

  /** Opens the row context menu from the keyboard, positioned on the selected row. */
  const openRowMenu = useCallback(() => {
    if (selectedPid == null || selectedIndex < 0) return;
    const fire = () => {
      const el = document.getElementById(rowDomId(selectedPid));
      if (!el) return;
      const rect = el.getBoundingClientRect();
      el.dispatchEvent(
        new globalThis.MouseEvent("contextmenu", {
          bubbles: true,
          cancelable: true,
          clientX: rect.left + 24,
          clientY: rect.top + rect.height / 2,
        }),
      );
    };
    if (document.getElementById(rowDomId(selectedPid))) {
      fire();
    } else {
      virtualizer.scrollToIndex(selectedIndex, { align: "auto" });
      requestAnimationFrame(fire);
    }
  }, [selectedPid, selectedIndex, virtualizer]);

  // A keyboard-generated contextmenu event targets the grid itself, which
  // has no row to act on; redirect it to the selected row.
  const onContextMenuCapture = useCallback(
    (e: MouseEvent<HTMLDivElement>) => {
      if (e.target !== e.currentTarget || selectedIndex < 0) return;
      e.preventDefault();
      e.stopPropagation();
      openRowMenu();
    },
    [openRowMenu, selectedIndex],
  );

  const onKeyDown = useCallback(
    (e: KeyboardEvent<HTMLDivElement>) => {
      if (items.length === 0) return;
      const current = items[selectedIndex];
      const page = Math.max(
        1,
        Math.floor((scrollRef.current?.clientHeight ?? 300) / rowHeight) - 1,
      );
      if ((e.key === "F10" && e.shiftKey) || e.key === "ContextMenu") {
        e.preventDefault();
        openRowMenu();
        return;
      }
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
      openRowMenu,
      toggleCollapsed,
      onRequestTerminate,
      virtualizer,
    ],
  );

  const find = useCallback(
    (pid: number) => items.find((i) => !i.group && i.row.pid === pid)?.row,
    [items],
  );

  const virtualItems = virtualizer.getVirtualItems();
  const activeDescendant =
    selectedPid != null && virtualItems.some((v) => v.index === selectedIndex)
      ? rowDomId(selectedPid)
      : undefined;

  const table = (
    <div
      ref={scrollRef}
      role={tree ? "treegrid" : "grid"}
      aria-label="Processes"
      aria-rowcount={items.length + 1}
      aria-colcount={visibleColumns.length}
      aria-activedescendant={activeDescendant}
      tabIndex={0}
      onKeyDown={onKeyDown}
      onContextMenuCapture={onContextMenuCapture}
      className="relative min-h-0 flex-1 overflow-auto rounded-lg border border-border bg-surface focus-visible:outline-2"
    >
      <div style={{ width: totalWidth, minWidth: "100%" }}>
        <HeaderRow columns={visibleColumns} template={template} sort={sort} />
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualItems.map((v) => {
            const item = items[v.index];
            if (!item) return null;
            const { row } = item;
            return (
              <Row
                key={row.pid}
                row={row}
                index={v.index}
                depth={item.depth ?? 0}
                hasChildren={item.hasChildren ?? false}
                expanded={item.expanded ?? false}
                group={item.group ?? false}
                columns={visibleColumns}
                template={template}
                nowSecs={rowNow}
                totalMemoryBytes={ctx.totalMemoryBytes}
                heat={heat}
                isNew={isNewProcess(row.startTime, ctx.nowSecs, highlightSecs)}
                tree={tree}
                selected={row.pid === selectedPid}
                isSelf={row.pid === selfPid}
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
