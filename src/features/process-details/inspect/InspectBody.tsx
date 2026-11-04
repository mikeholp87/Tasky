import type { ReactNode } from "react";
import { RefreshCw } from "lucide-react";
import { isBackendAvailable } from "@/services/ipc";
import type { AppError } from "@/types";

/**
 * Shared frame for the on-demand inspection tabs: toolbar slot, a refresh
 * button, and the loading / unsupported / denied / exited states.
 */
export function InspectBody({
  loading,
  error,
  hasData,
  onRefresh,
  toolbar,
  note,
  children,
}: {
  loading: boolean;
  error: AppError | null;
  hasData: boolean;
  onRefresh: () => void;
  toolbar?: ReactNode;
  note?: ReactNode;
  children: ReactNode;
}) {
  let body: ReactNode;
  if (!isBackendAvailable()) {
    body = <p className="text-fg-muted">Open the desktop app to inspect processes.</p>;
  } else if (error && !hasData) {
    body = (
      <p
        role={error.kind === "unsupportedOperation" ? "status" : "alert"}
        className="text-fg-muted"
      >
        {error.kind === "processNotFound" ? "This process has exited." : error.message}
      </p>
    );
  } else if (loading) {
    body = <p className="text-fg-muted">Loading…</p>;
  } else {
    body = children;
  }
  return (
    <div className="space-y-3 px-4 pb-4 pt-3">
      <div className="flex flex-wrap items-center gap-3">
        {toolbar}
        <button
          type="button"
          onClick={onRefresh}
          className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg"
        >
          <RefreshCw size="0.875rem" aria-hidden /> Refresh
        </button>
      </div>
      {note && <p className="text-fg-muted">{note}</p>}
      {error && hasData && (
        <p role="alert" className="text-warn">
          {error.message}
        </p>
      )}
      {body}
    </div>
  );
}
