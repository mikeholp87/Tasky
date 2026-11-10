/**
 * Loads persisted settings and keeps UI preferences that live in other
 * stores (process table columns) in sync with them.
 */
import { COLUMN_BY_ID, DEFAULT_VISIBLE_COLUMNS, type ColumnId } from "@/features/processes/logic";
import { useProcessStore } from "@/stores/processStore";
import { useSettingsStore } from "@/stores/settingsStore";
import type { Settings } from "@/types";
import { backend, isBackendAvailable, toAppError } from "./ipc";

const isColumn = (id: string): id is ColumnId => Object.hasOwn(COLUMN_BY_ID, id);

/** Restores table columns from settings, ignoring ids from older versions. */
export function columnsFromSettings(s: Pick<Settings, "visibleColumns" | "columnWidths">) {
  const visible = s.visibleColumns.filter(isColumn);
  const widths: Partial<Record<ColumnId, number>> = {};
  for (const [id, w] of Object.entries(s.columnWidths)) {
    if (isColumn(id) && typeof w === "number") widths[id] = w;
  }
  return {
    visibleColumns:
      visible.length > 0
        ? (["name", ...visible.filter((c) => c !== "name")] as ColumnId[])
        : [...DEFAULT_VISIBLE_COLUMNS],
    columnWidths: widths,
  };
}

let unsubscribe: (() => void) | undefined;
/** Set while one store is copying columns into the other, so the copy is not echoed back. */
let syncing = false;

type Columns = { visibleColumns: unknown; columnWidths: unknown };
const sameColumns = (a: Columns, b: Columns) =>
  a.visibleColumns === b.visibleColumns && a.columnWidths === b.columnWidths;

function sync(copy: () => void): void {
  if (syncing) return;
  syncing = true;
  try {
    copy();
  } finally {
    syncing = false;
  }
}

export async function loadSettings(): Promise<void> {
  if (!isBackendAvailable()) return;
  try {
    const settings = await backend.getSettings();
    useSettingsStore.getState().hydrate(settings);
    useProcessStore.setState(columnsFromSettings(settings));
  } catch (e) {
    console.warn("Could not load settings:", toAppError(e).message);
  }

  unsubscribe?.();
  const fromTable = useProcessStore.subscribe((state, prev) => {
    if (sameColumns(state, prev)) return;
    sync(() =>
      useSettingsStore.getState().update({
        visibleColumns: state.visibleColumns,
        columnWidths: Object.fromEntries(
          Object.entries(state.columnWidths).filter(([, w]) => w != null),
        ) as Record<string, number>,
      }),
    );
  });
  // Settings change on their own after a reset or a rolled-back save.
  const fromSettings = useSettingsStore.subscribe((state, prev) => {
    if (sameColumns(state, prev)) return;
    sync(() => useProcessStore.setState(columnsFromSettings(state)));
  });
  unsubscribe = () => {
    fromTable();
    fromSettings();
  };
}
