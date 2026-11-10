import { create } from "zustand";

export type ToastTone = "success" | "error" | "info";

export interface Toast {
  id: number;
  tone: ToastTone;
  message: string;
  /** Optional follow-up action, e.g. "Force end" after a failed close. Stays until used or dismissed. */
  action?: { label: string; run: () => void };
}

interface ToastState {
  toasts: Toast[];
  push: (toast: Omit<Toast, "id">) => void;
  dismiss: (id: number) => void;
  /** Stops a toast's dismissal countdown while the user is reading or using it. */
  pause: (id: number) => void;
  resume: (id: number) => void;
}

const DURATION_MS = { success: 4000, info: 4000, error: 8000 } as const;
const MAX_TOASTS = 4;
let nextId = 1;

interface Countdown {
  handle: ReturnType<typeof setTimeout> | undefined;
  remainingMs: number;
  startedAt: number;
}
const countdowns = new Map<number, Countdown>();

function startCountdown(id: number, c: Countdown): void {
  c.startedAt = Date.now();
  c.handle = setTimeout(() => useToastStore.getState().dismiss(id), c.remainingMs);
}

function clearCountdown(id: number): void {
  clearTimeout(countdowns.get(id)?.handle);
  countdowns.delete(id);
}

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  push: (toast) => {
    const id = nextId++;
    set((s) => {
      const kept = s.toasts.slice(-(MAX_TOASTS - 1));
      for (const t of s.toasts) if (!kept.includes(t)) clearCountdown(t.id);
      return { toasts: [...kept, { ...toast, id }] };
    });
    if (toast.action) return;
    const c: Countdown = { handle: undefined, remainingMs: DURATION_MS[toast.tone], startedAt: 0 };
    countdowns.set(id, c);
    startCountdown(id, c);
  },
  dismiss: (id) => {
    clearCountdown(id);
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
  },
  pause: (id) => {
    const c = countdowns.get(id);
    if (c?.handle === undefined) return;
    clearTimeout(c.handle);
    c.handle = undefined;
    c.remainingMs = Math.max(0, c.remainingMs - (Date.now() - c.startedAt));
  },
  resume: (id) => {
    const c = countdowns.get(id);
    if (c && c.handle === undefined) startCountdown(id, c);
  },
}));

export const toast = {
  success: (message: string) => useToastStore.getState().push({ tone: "success", message }),
  info: (message: string) => useToastStore.getState().push({ tone: "info", message }),
  error: (message: string, action?: Toast["action"]) =>
    useToastStore.getState().push({ tone: "error", message, action }),
};
