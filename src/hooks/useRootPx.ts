import { useSyncExternalStore } from "react";

/** The root font size in px, rounded to half a pixel, so JS-sized things (canvas, row heights) track the fluid type scale. */
function read(): number {
  const px = parseFloat(getComputedStyle(document.documentElement).fontSize);
  return Number.isFinite(px) ? Math.round(px * 2) / 2 : 16;
}

function subscribe(onChange: () => void): () => void {
  window.addEventListener("resize", onChange);
  return () => window.removeEventListener("resize", onChange);
}

export function useRootPx(): number {
  return useSyncExternalStore(subscribe, read, () => 16);
}

/** `base` px at the default 16px root, scaled to the current root size. */
export const scaled = (base: number, rootPx: number): number => Math.round((base * rootPx) / 16);
