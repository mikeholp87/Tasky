import { CheckCircle2, Info, TriangleAlert, X } from "lucide-react";
import { useEffect, useState } from "react";
import { cn } from "@/lib/cn";
import { useToastStore, type Toast } from "@/stores/toastStore";

const ICON = { success: CheckCircle2, info: Info, error: TriangleAlert } as const;

/**
 * Bottom-right notification stack. Errors are announced assertively, the rest
 * politely; the live regions stay mounted so additions are announced.
 */
export function Toaster() {
  const toasts = useToastStore((s) => s.toasts);
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[60] flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-2">
      <div aria-live="polite" className="flex flex-col gap-2">
        {toasts
          .filter((t) => t.tone !== "error")
          .map((t) => (
            <ToastItem key={t.id} toast={t} />
          ))}
      </div>
      <div aria-live="assertive" className="flex flex-col gap-2">
        {toasts
          .filter((t) => t.tone === "error")
          .map((t) => (
            <ToastItem key={t.id} toast={t} />
          ))}
      </div>
    </div>
  );
}

function ToastItem({ toast: t }: { toast: Toast }) {
  const dismiss = useToastStore((s) => s.dismiss);
  const pause = useToastStore((s) => s.pause);
  const resume = useToastStore((s) => s.resume);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const held = hovered || focused;
  useEffect(() => {
    if (held) pause(t.id);
    else resume(t.id);
  }, [held, t.id, pause, resume]);

  const Icon = ICON[t.tone];
  return (
    <div
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) setFocused(false);
      }}
      className="pointer-events-auto flex items-start gap-3 rounded-lg border border-border bg-surface p-3 shadow-lg"
    >
      <Icon
        size="1rem"
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
        <X size="0.875rem" />
      </button>
    </div>
  );
}
