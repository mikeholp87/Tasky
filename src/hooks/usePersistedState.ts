import { useCallback, useState } from "react";

/**
 * `useState` that survives restarts, for a view choice that is one of a few
 * known values. A stored value that is no longer allowed (an option removed
 * in an update, a hand-edited file) is ignored. Storage can be unavailable or
 * full, so every access is guarded and the app works without it.
 */
export function usePersistedState<T extends string | number>(
  key: string,
  initial: T,
  allowed: readonly T[],
): readonly [T, (next: T) => void] {
  const storageKey = `tm:${key}`;
  const [value, setValue] = useState<T>(() => {
    try {
      const stored: unknown = JSON.parse(localStorage.getItem(storageKey) ?? "null");
      return allowed.find((a) => a === stored) ?? initial;
    } catch {
      return initial;
    }
  });
  const set = useCallback(
    (next: T) => {
      setValue(next);
      try {
        localStorage.setItem(storageKey, JSON.stringify(next));
      } catch {
        // Not persisted; the choice still applies for this session.
      }
    },
    [storageKey],
  );
  return [value, set] as const;
}
