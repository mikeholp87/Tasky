import { Search } from "lucide-react";
import { Dialog } from "radix-ui";
import { useEffect, useMemo, useState } from "react";
import { cn } from "@/lib/cn";
import { refreshNow, togglePaused } from "@/services/monitoring";
import { useMetricsStore } from "@/stores/metricsStore";
import { useProcessStore } from "@/stores/processStore";
import { useUiStore } from "@/stores/uiStore";
import { ROUTES } from "./routes";

interface Command {
  id: string;
  label: string;
  run: () => void;
}

/** Ctrl/Cmd+K quick actions. */
export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const paused = useMetricsStore((s) => s.monitoring.paused);
  const navigate = useUiStore((s) => s.navigate);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const commands = useMemo((): Command[] => {
    const q = query.trim();
    const list: Command[] = [];
    if (q) {
      list.push({
        id: "search",
        label: `Search processes for "${q}"`,
        run: () => {
          navigate("processes");
          useProcessStore.getState().setSearch(q);
        },
      });
    }
    list.push(
      ...ROUTES.map((r) => ({
        id: `go-${r.id}`,
        label: `Go to ${r.label}`,
        run: () => navigate(r.id),
      })),
      {
        id: "pause",
        label: paused ? "Resume monitoring" : "Pause monitoring",
        run: () => void togglePaused(),
      },
      { id: "refresh", label: "Refresh now", run: () => void refreshNow() },
    );
    const needle = q.toLowerCase();
    return list.filter((c) => c.id === "search" || c.label.toLowerCase().includes(needle));
  }, [query, paused, navigate]);

  const close = () => {
    setOpen(false);
    setQuery("");
    setActive(0);
  };
  const run = (c: Command | undefined) => {
    if (!c) return;
    close();
    c.run();
  };

  return (
    <Dialog.Root open={open} onOpenChange={(o) => (o ? setOpen(true) : close())}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/30" />
        <Dialog.Content
          aria-describedby={undefined}
          className="fixed left-1/2 top-24 z-50 w-[520px] max-w-[calc(100vw-2rem)] -translate-x-1/2 overflow-hidden rounded-xl border border-border bg-surface shadow-xl"
        >
          <Dialog.Title className="sr-only">Command palette</Dialog.Title>
          <div className="flex items-center gap-2 border-b border-border px-3">
            <Search size={15} className="text-fg-subtle" aria-hidden />
            <input
              autoFocus
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setActive(0);
              }}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setActive((a) => Math.min(commands.length - 1, a + 1));
                } else if (e.key === "ArrowUp") {
                  e.preventDefault();
                  setActive((a) => Math.max(0, a - 1));
                } else if (e.key === "Enter") {
                  e.preventDefault();
                  run(commands[active]);
                }
              }}
              placeholder="Type a command or a process to search…"
              aria-label="Command"
              role="combobox"
              aria-expanded
              aria-controls="palette-list"
              aria-activedescendant={commands[active] ? `cmd-${commands[active].id}` : undefined}
              className="h-11 flex-1 bg-transparent outline-none placeholder:text-fg-subtle"
            />
          </div>
          <ul id="palette-list" role="listbox" className="max-h-80 overflow-auto p-1">
            {commands.map((c, i) => (
              <li
                key={c.id}
                id={`cmd-${c.id}`}
                role="option"
                aria-selected={i === active}
                onMouseEnter={() => setActive(i)}
                onClick={() => run(c)}
                className={cn("cursor-default rounded px-3 py-2", i === active && "bg-surface-2")}
              >
                {c.label}
              </li>
            ))}
            {commands.length === 0 && <li className="px-3 py-2 text-fg-muted">No matches.</li>}
          </ul>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
