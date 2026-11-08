import { History, Info, Play } from "lucide-react";
import { useEffect } from "react";
import { EmptyState } from "@/components/EmptyState";
import { PageHeader } from "@/components/PageHeader";
import { loadBenchmarkData } from "@/services/benchmarks";
import { useMetricsStore } from "@/stores/metricsStore";
import { isBackendAvailable } from "@/services/ipc";
import { isTerminal, latestByKind, useBenchmarkStore } from "@/stores/benchmarkStore";
import { formatTimestamp } from "@/utils/format";
import { AboutView } from "./components/AboutView";
import { CategoryCard } from "./components/CategoryCard";
import { CompareView } from "./components/CompareView";
import { CategoryView } from "./components/CategoryView";
import { HistoryView } from "./components/HistoryView";
import { PreflightDialog } from "./components/PreflightDialog";
import { ResultView } from "./components/ResultView";
import { RunPanel } from "./components/RunPanel";
import { linkButton, primaryButton, secondaryButton } from "./components/styles";
import { ALL_KINDS } from "./format";

function Home() {
  const history = useBenchmarkStore((s) => s.history);
  const run = useBenchmarkStore((s) => s.run);
  const setView = useBenchmarkStore((s) => s.setView);
  const latest = latestByKind(history);
  const last = history[0];
  const running = run !== null && !isTerminal(run.phase);

  return (
    <>
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        {ALL_KINDS.map((kind) => (
          <CategoryCard key={kind} kind={kind} latest={latest[kind]} />
        ))}
      </div>
      <div className="mt-4 flex flex-wrap items-center gap-3">
        <span className="text-fg-muted">
          {last ? `Last benchmark: ${formatTimestamp(last.startedAtMs)}` : "No benchmarks run yet."}
        </span>
        {last && (
          <button
            type="button"
            className={linkButton}
            onClick={() => setView({ name: "result", id: last.id })}
          >
            View results
          </button>
        )}
        {history.length > 1 && (
          <button type="button" className={linkButton} onClick={() => setView({ name: "compare" })}>
            Compare
          </button>
        )}
      </div>
      {!running && history.length === 0 && (
        <p className="mt-2 text-fg-muted">
          Benchmarks only run when you start them. The full benchmark runs CPU, GPU and storage
          tests; the internet test is optional.
        </p>
      )}
    </>
  );
}

export function BenchmarksPage() {
  const mounts = useMetricsStore((s) => s.tick?.disks.map((d) => d.mountPoint).join("\n"));
  const view = useBenchmarkStore((s) => s.view);
  const run = useBenchmarkStore((s) => s.run);
  const requestLaunch = useBenchmarkStore((s) => s.requestLaunch);
  const setView = useBenchmarkStore((s) => s.setView);
  const running = run !== null && !isTerminal(run.phase);

  useEffect(() => {
    void loadBenchmarkData();
  }, [mounts]);

  if (!isBackendAvailable()) {
    return (
      <>
        <PageHeader title="Benchmarks" description="Test your computer's performance." />
        <EmptyState title="Benchmarks need the desktop app">
          Benchmarks run in the Task Manager backend and are unavailable in a browser preview.
        </EmptyState>
      </>
    );
  }

  return (
    <>
      <PageHeader
        title="Benchmarks"
        description="Test your computer's performance. Results stay on this computer."
        actions={
          <>
            <button
              type="button"
              className={secondaryButton}
              onClick={() => setView({ name: "history" })}
            >
              <History size={14} aria-hidden />
              History
            </button>
            <button
              type="button"
              className={secondaryButton}
              onClick={() => setView({ name: "about" })}
            >
              <Info size={14} aria-hidden />
              About these tests
            </button>
            <button
              type="button"
              className={primaryButton}
              disabled={running}
              onClick={() => requestLaunch(["cpu", "gpu", "disk"])}
            >
              <Play size={14} aria-hidden />
              Run full benchmark
            </button>
          </>
        }
      />
      <RunPanel />
      {view.name === "home" && <Home />}
      {view.name === "category" && <CategoryView kind={view.kind} />}
      {view.name === "result" && <ResultView id={view.id} />}
      {view.name === "history" && <HistoryView />}
      {view.name === "compare" && (
        <CompareView baselineId={view.baselineId} currentId={view.currentId} />
      )}
      {view.name === "about" && <AboutView />}
      <PreflightDialog />
    </>
  );
}
