/// <reference types="node" />
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

// Vitest stubs .css imports to empty strings, so read the file directly.
const css = readFileSync(resolve(process.cwd(), "src/styles/index.css"), "utf8");

/** Token map for one theme block, e.g. ":root" or ".dark". */
function tokens(selector: string): Record<string, string> {
  const start = css.indexOf(`\n${selector} {`);
  const block = css.slice(start, css.indexOf("\n}", start));
  const out: Record<string, string> = {};
  for (const m of block.matchAll(/--([a-z0-9-]+):\s*(#[0-9a-f]{6})\s*;/gi)) {
    if (m[1] && m[2]) out[m[1]] = m[2];
  }
  return out;
}

function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

function contrast(a: string, b: string): number {
  const [la, lb] = [luminance(a), luminance(b)];
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** [foreground, background, minimum ratio]: 4.5 for text, 3 for graphics. */
const PAIRS: [string, string, number][] = [
  ["fg", "bg", 4.5],
  ["fg", "surface", 4.5],
  ["fg-muted", "bg", 4.5],
  ["fg-muted", "surface", 4.5],
  ["fg-muted", "surface-2", 4.5],
  ["fg-subtle", "bg", 4.5],
  ["fg-subtle", "surface", 4.5],
  ["fg-subtle", "surface-2", 4.5],
  ["accent", "surface", 4.5],
  ["accent", "bg", 4.5],
  ["accent-fg", "accent", 4.5],
  ["fg", "accent-soft", 4.5],
  ["ok", "surface", 4.5],
  ["warn", "surface", 4.5],
  ["warn", "warn-soft", 4.5],
  ["crit", "surface", 4.5],
  ["crit", "crit-soft", 4.5],
  ["warn-bar", "surface", 3],
  ["res-cpu", "surface", 3],
  ["res-mem", "surface", 3],
  ["res-disk", "surface", 3],
  ["res-net", "surface", 3],
  ["border-strong", "surface", 1.5],
];

describe.each([":root", ".dark"])("contrast in %s", (selector) => {
  const t = tokens(selector);
  it.each(PAIRS)("%s on %s is at least %s:1", (fg, bg, min) => {
    expect(t[fg], `--${fg} defined`).toBeDefined();
    expect(t[bg], `--${bg} defined`).toBeDefined();
    expect(contrast(t[fg] ?? "", t[bg] ?? "")).toBeGreaterThanOrEqual(min);
  });
});
