import { useMemo, useState, type FormEvent } from "react";
import { File, Folder, Search } from "lucide-react";
import { EmptyState } from "@/components/EmptyState";
import { PageHeader } from "@/components/PageHeader";
import { outlineButton } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/Card";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { useUiStore } from "@/stores/uiStore";
import type { AppError, FileUserList } from "@/types";
import { USAGE_LABEL, groupByProcess, summarize } from "./logic";

export function FileFinderPage() {
  const openProcess = useUiStore((s) => s.openProcess);
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<FileUserList | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const groups = useMemo(() => (result ? groupByProcess(result.uses) : []), [result]);

  async function search(target: string) {
    setBusy(true);
    setError(null);
    try {
      setResult(await backend.findFileUsers(target));
    } catch (e) {
      setResult(null);
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  }

  async function choose(directory: boolean) {
    try {
      const picked = await backend.pickPath(directory);
      if (picked) {
        setPath(picked);
        await search(picked);
      }
    } catch (e) {
      setError(toAppError(e));
    }
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    if (path.trim()) void search(path);
  }

  const available = isBackendAvailable();

  return (
    <>
      <PageHeader
        title="Find Process Using File"
        description="Why can't I delete this? Which program has it open?"
      />
      <form onSubmit={submit} className="mb-4 flex flex-wrap items-center gap-2">
        <input
          type="text"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="Type or paste a path, or choose one"
          aria-label="Path to search for"
          spellCheck={false}
          className="h-8 min-w-72 flex-1 rounded-md border border-border bg-surface px-2.5 font-mono text-[0.8125rem] outline-none placeholder:font-sans placeholder:text-fg-subtle focus:border-accent"
        />
        <button
          type="submit"
          className={outlineButton}
          disabled={!available || busy || !path.trim()}
        >
          <Search size="0.875rem" aria-hidden /> Find
        </button>
        <button
          type="button"
          className={outlineButton}
          disabled={!available || busy}
          onClick={() => void choose(false)}
        >
          <File size="0.875rem" aria-hidden /> Choose file…
        </button>
        <button
          type="button"
          className={outlineButton}
          disabled={!available || busy}
          onClick={() => void choose(true)}
        >
          <Folder size="0.875rem" aria-hidden /> Choose folder…
        </button>
      </form>

      {!available ? (
        <EmptyState title="Backend not available">Open the desktop app to search.</EmptyState>
      ) : error ? (
        <div role="alert" className="rounded-md bg-warn-soft px-4 py-2 text-warn">
          {error.message}
        </div>
      ) : busy ? (
        <EmptyState title="Searching…">
          Checking every process you are allowed to inspect.
        </EmptyState>
      ) : !result ? (
        <EmptyState title="Nothing searched yet">
          Pick a file or folder to see which processes hold it open, run from it, or map it into
          memory.
        </EmptyState>
      ) : (
        <div className="space-y-3">
          <p role="status" className="font-medium">
            {summarize(groups.length, result.isDirectory)}
          </p>
          <p className="break-all font-mono text-[0.8125rem] text-fg-muted">
            {result.resolvedPath}
          </p>
          {result.note && (
            <p role="note" className="text-fg-muted">
              {result.note}
            </p>
          )}
          {result.inaccessibleProcesses > 0 && (
            <p role="note" className="text-fg-muted">
              {result.inaccessibleProcesses} processes could not be inspected (they belong to other
              accounts), so they may also be using it. Run as administrator/root to include them.
            </p>
          )}
          {result.truncated && (
            <p role="note" className="text-warn">
              The search stopped early because it took too long or found too many matches. Results
              may be incomplete.
            </p>
          )}
          {groups.map((g) => (
            <Card key={g.pid}>
              <CardHeader
                title={
                  <>
                    {g.name || "Unknown"}{" "}
                    <span className="num font-normal text-fg-muted">PID {g.pid}</span>
                  </>
                }
                action={
                  <button
                    type="button"
                    className={outlineButton}
                    onClick={() => openProcess(g.pid)}
                  >
                    Open process details
                  </button>
                }
              />
              <ul className="space-y-1 px-4 pb-3 pt-2">
                {g.uses.map((u) => (
                  <li key={`${u.usage}:${u.path}`} className="grid grid-cols-[150px_1fr] gap-3">
                    <span className="text-fg-muted">{USAGE_LABEL[u.usage]}</span>
                    <code className="selectable break-all font-mono text-[0.8125rem]">
                      {u.path}
                    </code>
                  </li>
                ))}
              </ul>
            </Card>
          ))}
        </div>
      )}
    </>
  );
}
