import { create } from "zustand";
import {
  COLUMNS,
  COLUMN_BY_ID,
  DEFAULT_VISIBLE_COLUMNS,
  type ColumnId,
  type FilterPreset,
  type SortState,
  nextSort,
} from "@/features/processes/logic";

export type ProcessViewMode = "list" | "tree" | "apps";

interface ProcessState {
  search: string;
  preset: FilterPreset;
  sort: SortState;
  mode: ProcessViewMode;
  visibleColumns: ColumnId[];
  columnWidths: Partial<Record<ColumnId, number>>;
  selectedPid: number | null;
  /** Bumped to ask the search box to take focus (Ctrl/Cmd+F). */
  focusSearchToken: number;
  /** Tree mode: PIDs whose children are hidden (everything else is expanded). */
  collapsed: ReadonlySet<number>;
  /** Applications mode: group ids that are expanded (groups start collapsed). */
  expandedApps: ReadonlySet<number>;

  setSearch: (search: string) => void;
  setPreset: (preset: FilterPreset) => void;
  sortBy: (column: ColumnId) => void;
  setMode: (mode: ProcessViewMode) => void;
  toggleColumn: (column: ColumnId) => void;
  resetColumns: () => void;
  setColumnWidth: (column: ColumnId, width: number) => void;
  select: (pid: number | null) => void;
  requestSearchFocus: () => void;
  toggleCollapsed: (pid: number) => void;
  setCollapsed: (pid: number, collapsed: boolean) => void;
  collapseAll: (pids: Iterable<number>) => void;
  expandAll: () => void;
  setExpandedApps: (ids: Iterable<number>) => void;
}

function flipped(set: ReadonlySet<number>, id: number): Set<number> {
  const next = new Set(set);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

export const useProcessStore = create<ProcessState>((set) => ({
  search: "",
  preset: "all",
  sort: { column: "cpu", direction: "desc" },
  mode: "list",
  visibleColumns: [...DEFAULT_VISIBLE_COLUMNS],
  columnWidths: {},
  selectedPid: null,
  focusSearchToken: 0,
  collapsed: new Set(),
  expandedApps: new Set(),

  setSearch: (search) => set({ search }),
  setPreset: (preset) => set({ preset }),
  sortBy: (column) => set((s) => ({ sort: nextSort(s.sort, column) })),
  setMode: (mode) => set({ mode }),
  toggleColumn: (column) =>
    set((s) => {
      // The name column anchors the table and cannot be hidden.
      if (column === "name") return s;
      const visible = new Set(s.visibleColumns);
      if (visible.has(column)) visible.delete(column);
      else visible.add(column);
      return { visibleColumns: COLUMNS.map((c) => c.id).filter((id) => visible.has(id)) };
    }),
  resetColumns: () => set({ visibleColumns: [...DEFAULT_VISIBLE_COLUMNS], columnWidths: {} }),
  setColumnWidth: (column, width) =>
    set((s) => ({
      columnWidths: {
        ...s.columnWidths,
        [column]: Math.max(COLUMN_BY_ID[column].minWidth, Math.round(width)),
      },
    })),
  select: (selectedPid) => set({ selectedPid }),
  requestSearchFocus: () => set((s) => ({ focusSearchToken: s.focusSearchToken + 1 })),
  // The tree starts expanded (`collapsed` lists exceptions); the applications
  // view starts collapsed (`expandedApps` lists exceptions). Same row actions.
  toggleCollapsed: (pid) =>
    set((s) =>
      s.mode === "apps"
        ? { expandedApps: flipped(s.expandedApps, pid) }
        : { collapsed: flipped(s.collapsed, pid) },
    ),
  setCollapsed: (pid, collapsed) =>
    set((s) => {
      if (s.mode === "apps") {
        if (s.expandedApps.has(pid) === !collapsed) return s;
        return { expandedApps: flipped(s.expandedApps, pid) };
      }
      if (s.collapsed.has(pid) === collapsed) return s;
      return { collapsed: flipped(s.collapsed, pid) };
    }),
  collapseAll: (pids) => set({ collapsed: new Set(pids) }),
  expandAll: () => set({ collapsed: new Set() }),
  setExpandedApps: (ids) => set({ expandedApps: new Set(ids) }),
}));

export function columnWidth(widths: Partial<Record<ColumnId, number>>, column: ColumnId): number {
  return widths[column] ?? COLUMN_BY_ID[column].width;
}
