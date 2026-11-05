import { describe, expect, it } from "vitest";
import type { StartupItem } from "@/types";
import { filterItems } from "./logic";

const item = (name: string, enabled: boolean, command = name): StartupItem => ({
  id: `${name}.desktop`,
  name,
  comment: null,
  command,
  scope: "user",
  enabled,
  onlyIn: null,
  runsHere: true,
});

describe("startup filter", () => {
  const items = [item("Zoom", true), item("Dropbox", false), item("Agent", true, "/usr/bin/sync")];

  it("lists items by name, whether or not they are enabled", () => {
    expect(filterItems(items, "").map((i) => i.name)).toEqual(["Agent", "Dropbox", "Zoom"]);
  });

  it("keeps a row in place when it is switched on or off", () => {
    const flipped = items.map((i) => (i.name === "Dropbox" ? { ...i, enabled: true } : i));
    expect(filterItems(flipped, "").map((i) => i.name)).toEqual(
      filterItems(items, "").map((i) => i.name),
    );
  });

  it("matches name or command, ignoring case", () => {
    expect(filterItems(items, " DROP ").map((i) => i.name)).toEqual(["Dropbox"]);
    expect(filterItems(items, "usr/bin").map((i) => i.name)).toEqual(["Agent"]);
  });
});
