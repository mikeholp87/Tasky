// Configuration regressions can silently broaden IPC authority without
// changing a handler. Check the actual build manifest, not a separate list.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(resolve(process.cwd(), path), "utf8");

describe("production security boundaries", () => {
  it("scopes every registered application command to the main capability", () => {
    const registered = [...read("src-tauri/src/lib.rs").matchAll(/commands::\w+::(\w+),/g)]
      .map((m) => m[1])
      .sort();
    const names = (source: string) =>
      [...source.matchAll(/^\s*"(\w+)",/gm)].map((m) => m[1]).sort();
    expect(registered.length).toBeGreaterThan(0);
    expect(names(read("src-tauri/build.rs"))).toEqual(registered);
    expect(names(read("src-tauri/permissions/main.toml"))).toEqual(registered);
    const config = JSON.parse(read("src-tauri/tauri.conf.json"));
    const capability = JSON.parse(read("src-tauri/capabilities/default.json"));
    expect(config.app.security.capabilities).toEqual(["default"]);
    expect(capability.windows).toEqual(["main"]);
    expect(capability.remote).toBeUndefined();
    expect(capability.permissions).toEqual([
      "core:event:allow-listen",
      "core:event:allow-unlisten",
      "main-commands",
    ]);
  });

  it("bundles scripts and denies remote execution, frames, forms and production devtools", () => {
    const config = JSON.parse(read("src-tauri/tauri.conf.json"));
    const csp: string = config.app.security.csp;
    for (const directive of [
      "script-src 'self'",
      "object-src 'none'",
      "frame-src 'none'",
      "base-uri 'none'",
      "form-action 'none'",
    ]) {
      expect(csp.split("; ")).toContain(directive);
    }
    expect(csp).not.toMatch(/unsafe-eval|https:\/\//);
    expect(
      config.app.windows.every((window: { devtools: boolean }) => window.devtools === false),
    ).toBe(true);
    expect(read("src-tauri/src/lib.rs")).toContain("security::navigation_allowed(url)");
  });
});
