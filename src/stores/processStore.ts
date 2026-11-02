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

export type ProcessViewMode = "list" | "tree";

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
  toggleCollapsed: (pid) =>
    set((s) => {
      const next = new Set(s.collapsed);
      if (next.has(pid)) next.delete(pid);
      else next.add(pid);
      return { collapsed: next };
    }),
  setCollapsed: (pid, collapsed) =>
    set((s) => {
      if (s.collapsed.has(pid) === collapsed) return s;
      const next = new Set(s.collapsed);
      if (collapsed) next.add(pid);
      else next.delete(pid);
      return { collapsed: next };
    }),
  collapseAll: (pids) => set({ collapsed: new Set(pids) }),
  expandAll: () => set({ collapsed: new Set() }),
}));

export function columnWidth(widths: Partial<Record<ColumnId, number>>, column: ColumnId): number {
  return widths[column] ?? COLUMN_BY_ID[column].width;
}
