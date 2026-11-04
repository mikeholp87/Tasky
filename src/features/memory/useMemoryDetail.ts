import { useEffect } from "react";
import { backend, isBackendAvailable } from "@/services/ipc";

let users = 0;
const send = (enabled: boolean) => void backend.setMemoryDetail(enabled).catch(() => undefined);

/**
 * Asks the engine for per-process proportional memory (extra reads) while a
 * component that shows it is on screen. Counted, so two such components can
 * overlap without one switching it off under the other.
 */
export function useMemoryDetail(): void {
  useEffect(() => {
    if (!isBackendAvailable()) return;
    if (users++ === 0) send(true);
    return () => {
      if (--users === 0) send(false);
    };
  }, []);
}
