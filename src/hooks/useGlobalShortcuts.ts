import { useEffect } from "react";
import { refreshNow, togglePaused } from "@/services/monitoring";
import { useProcessStore } from "@/stores/processStore";
import { useUiStore } from "@/stores/uiStore";

/** True when the event target is a control where typing/space has meaning. */
function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return (
    ["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(target.tagName) ||
    target.getAttribute("role") === "menuitem"
  );
}

/**
 * Application-wide shortcuts:
 * - Ctrl/Cmd+F: search processes
 * - Ctrl/Cmd+R: refresh now (and stop the webview from reloading)
 * - Space: pause/resume monitoring
 */
export function useGlobalShortcuts(): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      const key = e.key.toLowerCase();

      if (mod && key === "f") {
        e.preventDefault();
        useUiStore.getState().navigate("processes");
        useProcessStore.getState().requestSearchFocus();
        return;
      }
      if (mod && key === "r") {
        e.preventDefault();
        void refreshNow();
        return;
      }
      if (e.key === " " && !mod && !e.altKey && !isEditableTarget(e.target)) {
        e.preventDefault();
        void togglePaused();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
