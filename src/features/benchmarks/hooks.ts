import { useEffect, useState } from "react";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import type { BenchmarkRecord } from "@/types";

interface Loaded {
  id: string;
  record: BenchmarkRecord | null;
  error: string | null;
}

/** Loads a stored result; stale responses for a previous id are ignored. */
export function useBenchmarkRecord(id: string | null) {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  useEffect(() => {
    if (!id || !isBackendAvailable()) return;
    let cancelled = false;
    backend
      .getBenchmarkResult(id)
      .then((record) => !cancelled && setLoaded({ id, record, error: null }))
      .catch(
        (e: unknown) => !cancelled && setLoaded({ id, record: null, error: toAppError(e).message }),
      );
    return () => {
      cancelled = true;
    };
  }, [id]);
  const current = loaded && loaded.id === id ? loaded : null;
  return { record: current?.record ?? null, error: current?.error ?? null };
}
