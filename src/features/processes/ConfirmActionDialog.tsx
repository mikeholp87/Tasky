import { OctagonX, TriangleAlert } from "lucide-react";
import { AlertDialog } from "radix-ui";
import { cn } from "@/lib/cn";
import type { ProcessAction } from "@/types";
import { PRIORITY_LABELS, cancelPending, confirmPending, useActionStore } from "./actions";

interface Copy {
  title: string;
  body: string;
  confirm: string;
  danger: boolean;
}

function copyFor(action: ProcessAction, name: string): Copy {
  switch (action.type) {
    case "terminate":
      return {
        title: `End ${name}?`,
        body: "The process will be asked to close. It may contain unsaved data and may prompt you to save it.",
        confirm: "End process",
        danger: true,
      };
    case "forceKill":
      return {
        title: `Force end ${name}?`,
        body: "The process will be stopped immediately without a chance to save. Any unsaved work in it will be lost, and files it was writing may be left incomplete.",
        confirm: "Force end",
        danger: true,
      };
    case "suspend":
      return {
        title: `Suspend ${name}?`,
        body: "The process will be frozen until you resume it. Programs that depend on it may stop responding.",
        confirm: "Suspend",
        danger: false,
      };
    case "resume":
      return { title: `Resume ${name}?`, body: "", confirm: "Resume", danger: false };
    case "setPriority":
      return {
        title: `Set ${name} to ${PRIORITY_LABELS[action.priority]} priority?`,
        body: "",
        confirm: "Change priority",
        danger: false,
      };
  }
}

export function ConfirmActionDialog() {
  const pending = useActionStore((s) => s.pending);
  const busy = useActionStore((s) => s.busy);
  if (!pending) return null;

  const { row, action, assessment } = pending;
  const copy = copyFor(action, row.name);
  const blocked = assessment.level === "blocked";
  const warned = assessment.level === "warning";

  return (
    <AlertDialog.Root open onOpenChange={(open) => !open && cancelPending()}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="fixed inset-0 z-50 bg-black/40" />
        <AlertDialog.Content className="fixed left-1/2 top-1/2 z-50 w-[440px] max-w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border bg-surface p-5 shadow-xl">
          <AlertDialog.Title className="text-base font-semibold">
            {blocked ? `${row.name} is protected` : copy.title}
          </AlertDialog.Title>
          <AlertDialog.Description asChild>
            <div className="mt-2 space-y-3 text-fg-muted">
              <p className="num">
                PID {row.pid}
                {row.user ? ` · ${row.user}` : ""}
              </p>
              {!blocked && copy.body && <p>{copy.body}</p>}
              {assessment.reasons.length > 0 && (
                <ul
                  className={cn(
                    "space-y-1 rounded-md p-3",
                    blocked ? "bg-crit-soft text-crit" : "bg-warn-soft text-warn",
                  )}
                >
                  {assessment.reasons.map((r) => (
                    <li key={r} className="flex gap-2">
                      {blocked ? (
                        <OctagonX size={15} className="mt-0.5 shrink-0" aria-hidden />
                      ) : (
                        <TriangleAlert size={15} className="mt-0.5 shrink-0" aria-hidden />
                      )}
                      <span>{r}</span>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </AlertDialog.Description>
          <div className="mt-5 flex justify-end gap-2">
            <AlertDialog.Cancel asChild>
              <button
                type="button"
                disabled={busy}
                className="h-8 rounded-md border border-border px-3 hover:bg-surface-2"
              >
                {blocked ? "Close" : "Cancel"}
              </button>
            </AlertDialog.Cancel>
            {!blocked && (
              <button
                type="button"
                disabled={busy}
                onClick={() => void confirmPending()}
                className={cn(
                  "h-8 rounded-md px-3 font-medium disabled:opacity-60",
                  copy.danger || warned ? "bg-danger text-white" : "bg-accent text-accent-fg",
                )}
              >
                {busy ? "Working…" : warned ? `${copy.confirm} anyway` : copy.confirm}
              </button>
            )}
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
