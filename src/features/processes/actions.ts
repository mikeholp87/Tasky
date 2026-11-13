/**
 * Process action flow: assess → (confirm) → perform → report.
 *
 * Protection rules are enforced by the backend; this module only decides
 * when to ask the user and how to present results. Nothing here can bypass
 * a "blocked" verdict.
 */
import { create } from "zustand";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";
import type { PriorityLevel, ProcessAction, ProcessRow, ProtectionAssessment } from "@/types";

export interface PendingAction {
  row: ProcessRow;
  action: ProcessAction;
  assessment: ProtectionAssessment;
}

interface ActionState {
  pending: PendingAction | null;
  busy: boolean;
}

export const useActionStore = create<ActionState>(() => ({ pending: null, busy: false }));

/** Actions that always need explicit confirmation, even when allowed. */
export function needsConfirmation(action: ProcessAction): boolean {
  return action.type === "terminate" || action.type === "forceKill";
}

export const PRIORITY_LABELS: Record<PriorityLevel, string> = {
  idle: "Low",
  belowNormal: "Below normal",
  normal: "Normal",
  aboveNormal: "Above normal",
  high: "High",
};

/** Bumped per request so only the newest assessment opens a dialog or runs. */
let latestRequest = 0;

/** Starts an action. Shows the confirmation dialog when required. */
export async function requestAction(row: ProcessRow, action: ProcessAction): Promise<void> {
  if (!isBackendAvailable() || useActionStore.getState().busy) return;
  const request = ++latestRequest;
  let assessment: ProtectionAssessment;
  try {
    assessment = await backend.assessProcessAction(row.pid, action);
  } catch (e) {
    if (request === latestRequest) toast.error(toAppError(e).message);
    return;
  }
  if (request !== latestRequest || useActionStore.getState().busy) return;
  if (assessment.level !== "allowed" || needsConfirmation(action)) {
    useActionStore.setState({ pending: { row, action, assessment } });
    return;
  }
  await perform(row, action, false);
}

export function cancelPending(): void {
  if (!useActionStore.getState().busy) useActionStore.setState({ pending: null });
}

/** Confirms the pending dialog. Blocked actions cannot be confirmed. */
export async function confirmPending(): Promise<void> {
  const { pending, busy } = useActionStore.getState();
  if (!pending || busy || pending.assessment.level === "blocked") return;
  useActionStore.setState({ busy: true });
  await perform(pending.row, pending.action, pending.assessment.level === "warning");
  useActionStore.setState((s) => ({
    busy: false,
    pending: s.pending === pending ? null : s.pending,
  }));
}

async function perform(row: ProcessRow, action: ProcessAction, acknowledge: boolean) {
  try {
    const outcome = await backend.performProcessAction(row.pid, row.startTime, action, acknowledge);
    toast.success(outcome.message);
  } catch (e) {
    const err = toAppError(e);
    // A graceful close was impossible: offer the forceful alternative,
    // which goes through its own confirmation.
    if (err.kind === "actionUnavailable" && action.type === "terminate") {
      toast.error(err.message, {
        label: "Force end…",
        run: () => void requestAction(row, { type: "forceKill" }),
      });
    } else {
      toast.error(err.message);
    }
  }
}

export async function revealExecutable(row: ProcessRow): Promise<void> {
  try {
    await backend.revealExecutable(row.pid);
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}

export async function copyText(label: string, text: string | null | undefined): Promise<void> {
  if (!text) {
    toast.error(`${label} is unavailable for this process.`);
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
    toast.info(`Copied ${label.toLowerCase()}.`);
  } catch {
    toast.error("Could not access the clipboard.");
  }
}
