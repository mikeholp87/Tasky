import { describe, expect, it } from "vitest";
import { DEFAULT_VISIBLE_COLUMNS } from "@/features/processes/logic";
import { columnsFromSettings } from "./settings";

describe("columnsFromSettings", () => {
  it("falls back to defaults when nothing is stored", () => {
    expect(columnsFromSettings({ visibleColumns: [], columnWidths: {} }).visibleColumns).toEqual(
      DEFAULT_VISIBLE_COLUMNS,
    );
  });

  it("drops unknown ids and always keeps the name column first", () => {
    const r = columnsFromSettings({
      visibleColumns: ["cpu", "bogus", "pid"],
      columnWidths: { cpu: 120, nope: 50 },
    });
    expect(r.visibleColumns).toEqual(["name", "cpu", "pid"]);
    expect(r.columnWidths).toEqual({ cpu: 120 });
  });
});
