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
});

describe("startup filter", () => {
  const items = [item("Zoom", true), item("Dropbox", false), item("Agent", true, "/usr/bin/sync")];

  it("lists enabled items first, then by name", () => {
    expect(filterItems(items, "").map((i) => i.name)).toEqual(["Agent", "Zoom", "Dropbox"]);
  });

  it("matches name or command, ignoring case", () => {
    expect(filterItems(items, " DROP ").map((i) => i.name)).toEqual(["Dropbox"]);
    expect(filterItems(items, "usr/bin").map((i) => i.name)).toEqual(["Agent"]);
  });
});
