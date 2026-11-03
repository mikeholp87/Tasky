import { ArrowLeft } from "lucide-react";
import { Card } from "@/components/ui/Card";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import { secondaryButton } from "./styles";

const ABOUT: { q: string; a: string }[] = [
  {
    q: "What does CPU Single-Core measure?",
    a: "How fast one processor core runs integer, floating-point, hashing and compression work on one thread. It reflects how responsive everyday, lightly threaded programs feel.",
  },
  {
    q: "What does CPU Multi-Core measure?",
    a: "The same work running on every logical processor at once. Scaling shows how much faster all cores are than one; it is below the core count because of shared caches, memory and power limits.",
  },
  {
    q: "What does GPU Compute measure?",
    a: "How many floating-point operations per second the graphics processor completes in a compute shader with a known amount of work. It is a measured synthetic result, not the manufacturer's theoretical peak.",
  },
  {
    q: "What does Sequential Read measure?",
    a: "How quickly the drive delivers a large file read in order, in megabytes per second (MB/s). Sequential write is the same for writing.",
  },
  {
    q: "What are IOPS?",
    a: "Input/output operations per second: how many small (4 KiB) reads or writes at random places the drive completes each second. It matters for starting programs and handling many small files.",
  },
  {
    q: "What is latency?",
    a: "How long one operation takes. For storage it is the time for a single 4 KiB request (in microseconds); for the internet it is the round-trip time to the test server (in milliseconds). Lower is better.",
  },
  {
    q: "What is jitter?",
    a: "How much latency varies from one measurement to the next: the average difference between consecutive round trips. High jitter disrupts calls and games.",
  },
  {
    q: "What is loaded latency?",
    a: "Latency measured while the connection is busy downloading or uploading. A large increase over idle latency means a busy connection makes everything else feel slow.",
  },
  {
    q: "Why can benchmark scores change?",
    a: "Background programs, power settings, battery versus mains power, temperature, driver and system updates, and free drive space all affect results. Each result shows its variation and stability so you can judge how repeatable it was.",
  },
  {
    q: "Why does temperature matter?",
    a: "When a processor gets hot it lowers its clock speed to protect itself. Long tests can therefore be slower than short ones; the Extended profile measures this as sustained performance.",
  },
  {
    q: "Why can SSD cache affect results?",
    a: "Operating systems keep recently used file data in memory, and many SSDs absorb writes in a fast cache before slower flash. The storage test bypasses the system cache where possible and labels results when it cannot; short write tests may still be absorbed by the drive's own cache.",
  },
  {
    q: "How are scores calculated?",
    a: "Each measurement is divided by a fixed reference value (a score of 1000 is roughly the development laptop) and combined with a geometric mean. Raw measurements are always kept; scores are only a summary. Internet results are not part of the system score, and these scores are not comparable with other benchmark products.",
  },
];

export function AboutView() {
  const setView = useBenchmarkStore((s) => s.setView);
  return (
    <div className="max-w-3xl space-y-4">
      <button type="button" className={secondaryButton} onClick={() => setView({ name: "home" })}>
        <ArrowLeft size={14} aria-hidden />
        Benchmarks
      </button>
      <h2 className="text-base font-semibold">About these tests</h2>
      <Card className="divide-y divide-border">
        {ABOUT.map(({ q, a }) => (
          <section key={q} className="px-4 py-3">
            <h3 className="font-medium">{q}</h3>
            <p className="mt-1 text-fg-muted">{a}</p>
          </section>
        ))}
      </Card>
    </div>
  );
}
