import { create } from "zustand";

export type ToastTone = "success" | "error" | "info";

export interface Toast {
  id: number;
  tone: ToastTone;
  message: string;
  /** Optional follow-up action, e.g. "Force end" after a failed close. */
  action?: { label: string; run: () => void };
}

interface ToastState {
  toasts: Toast[];
  push: (toast: Omit<Toast, "id">) => void;
  dismiss: (id: number) => void;
}

const DURATION_MS = { success: 4000, info: 4000, error: 8000 } as const;
let nextId = 1;

export const useToastStore = create<ToastState>((set, get) => ({
  toasts: [],
  push: (toast) => {
    const id = nextId++;
    set((s) => ({ toasts: [...s.toasts.slice(-3), { ...toast, id }] }));
    setTimeout(() => get().dismiss(id), DURATION_MS[toast.tone]);
  },
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

export const toast = {
  success: (message: string) => useToastStore.getState().push({ tone: "success", message }),
  info: (message: string) => useToastStore.getState().push({ tone: "info", message }),
  error: (message: string, action?: Toast["action"]) =>
    useToastStore.getState().push({ tone: "error", message, action }),
};
