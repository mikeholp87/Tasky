import { CheckCircle2, Info, TriangleAlert, X } from "lucide-react";
import { cn } from "@/lib/cn";
import { useToastStore } from "@/stores/toastStore";

const ICON = { success: CheckCircle2, info: Info, error: TriangleAlert } as const;

/** Bottom-right notification stack. Announced politely to screen readers. */
export function Toaster() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);
  return (
    <div
      aria-live="polite"
      className="pointer-events-none fixed bottom-4 right-4 z-[60] flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-2"
    >
      {toasts.map((t) => {
        const Icon = ICON[t.tone];
        return (
          <div
            key={t.id}
            role={t.tone === "error" ? "alert" : "status"}
            className="pointer-events-auto flex items-start gap-3 rounded-lg border border-border bg-surface p-3 shadow-lg"
          >
            <Icon
              size={16}
              aria-hidden
              className={cn(
                "mt-0.5 shrink-0",
                t.tone === "success" && "text-ok",
                t.tone === "error" && "text-crit",
                t.tone === "info" && "text-fg-muted",
              )}
            />
            <div className="min-w-0 flex-1">
              <p>{t.message}</p>
              {t.action && (
                <button
                  type="button"
                  onClick={() => {
                    t.action?.run();
                    dismiss(t.id);
                  }}
                  className="mt-1 font-medium text-accent hover:underline"
                >
                  {t.action.label}
                </button>
              )}
            </div>
            <button
              type="button"
              aria-label="Dismiss"
              onClick={() => dismiss(t.id)}
              className="shrink-0 rounded text-fg-subtle hover:text-fg"
            >
              <X size={14} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
