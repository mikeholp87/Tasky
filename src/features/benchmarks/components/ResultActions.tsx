import { Copy, Download, GitCompare } from "lucide-react";
import { DropdownMenu } from "radix-ui";
import { backend, toAppError } from "@/services/ipc";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import { toast } from "@/stores/toastStore";
import { previousComparable } from "../logic";
import { secondaryButton } from "./styles";

const itemCls =
  "flex cursor-default items-center gap-2 rounded px-2 py-1.5 outline-none data-[highlighted]:bg-surface-2";

const FORMATS = [
  { id: "json", label: "JSON (complete, raw samples)" },
  { id: "csv", label: "CSV (one row per measurement)" },
  { id: "markdown", label: "Report (Markdown)" },
] as const;

export function ResultActions({ id }: { id: string }) {
  const history = useBenchmarkStore((s) => s.history);
  const setView = useBenchmarkStore((s) => s.setView);
  const previous = previousComparable(history, id);

  const exportAs = async (format: (typeof FORMATS)[number]["id"]) => {
    try {
      const path = await backend.exportBenchmark(id, format);
      if (path) toast.success(`Benchmark saved to ${path}`);
    } catch (e) {
      toast.error(toAppError(e).message);
    }
  };

  const copy = async () => {
    try {
      const text = await backend.benchmarkSummaryText(id);
      await navigator.clipboard.writeText(text);
      toast.success("Benchmark summary copied.");
    } catch (e) {
      toast.error(`Could not copy the summary: ${toAppError(e).message}`);
    }
  };

  return (
    <div className="flex flex-wrap gap-2">
      <button
        type="button"
        className={secondaryButton}
        disabled={!previous}
        title={previous ? undefined : "No earlier comparable result"}
        onClick={() =>
          previous && setView({ name: "compare", baselineId: previous.id, currentId: id })
        }
      >
        <GitCompare size={14} aria-hidden />
        Compare with previous
      </button>
      <button type="button" className={secondaryButton} onClick={() => void copy()}>
        <Copy size={14} aria-hidden />
        Copy summary
      </button>
      <DropdownMenu.Root>
        <DropdownMenu.Trigger asChild>
          <button type="button" className={secondaryButton}>
            <Download size={14} aria-hidden />
            Export
          </button>
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content
            align="end"
            sideOffset={4}
            className="z-50 w-72 rounded-lg border border-border bg-surface p-1 text-[13px] shadow-lg"
          >
            {FORMATS.map((f) => (
              <DropdownMenu.Item
                key={f.id}
                className={itemCls}
                onSelect={() => void exportAs(f.id)}
              >
                {f.label}
              </DropdownMenu.Item>
            ))}
            <p className="px-2 pb-1 pt-1 text-[12px] text-fg-muted">
              Exports contain hardware details and measurements only — no files or process
              information.
            </p>
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
    </div>
  );
}
