import { useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { SearchBox } from "@/components/SearchBox";
import { useRemoteData } from "@/hooks/useRemoteData";
import { scaled, useRootPx } from "@/hooks/useRootPx";
import { backend } from "@/services/ipc";
import type { MemoryMap, MemoryRegionKind } from "@/types";
import { formatBytes } from "@/utils/format";
import { InspectBody } from "./InspectBody";
import { REGION_COLOR, REGION_LABEL, filterRegions, segments } from "./logic";

const ROW_HEIGHT = 24;
const COLUMNS = "grid-cols-[9.375rem_9.375rem_4.375rem_5rem_8.75rem_1fr]";

export function MemoryMapTab({ pid }: { pid: number }) {
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<MemoryRegionKind | "all">("all");
  const { data, error, loading, refresh } = useRemoteData(
    () => backend.getMemoryMap(pid),
    `maps:${pid}`,
  );
  const shown = useMemo(() => filterRegions(data?.regions ?? [], kind, query), [data, kind, query]);
  return (
    <InspectBody
      loading={loading}
      error={error}
      hasData={data != null}
      onRefresh={refresh}
      toolbar={
        <>
          <SearchBox
            value={query}
            onChange={setQuery}
            label="Search memory map"
            placeholder="Path, address, permissions…"
          />
          <select
            aria-label="Region type"
            value={kind}
            onChange={(e) => setKind(e.target.value as MemoryRegionKind | "all")}
            className="h-8 rounded-md border border-border bg-surface px-2"
          >
            <option value="all">All regions</option>
            {(Object.keys(REGION_LABEL) as MemoryRegionKind[]).map((k) => (
              <option key={k} value={k}>
                {REGION_LABEL[k]}
              </option>
            ))}
          </select>
          {data && (
            <span className="text-fg-muted">
              {shown.length} of {data.regions.length}
            </span>
          )}
        </>
      }
    >
      {data && <Summary map={data} />}
      <RegionTable regions={shown} />
    </InspectBody>
  );
}

function Summary({ map }: { map: MemoryMap }) {
  const parts = segments(map.summary);
  if (parts.length === 0) return null;
  return (
    <div className="space-y-2">
      <p className="text-fg-muted">
        Address space mapped: <span className="num text-fg">{formatBytes(map.totalBytes)}</span> ·
        executable <span className="num text-fg">{formatBytes(map.executableBytes)}</span>. These
        are mapped (virtual) sizes, not memory in use; a mapping only costs RAM for the pages
        touched.
      </p>
      <div
        role="img"
        aria-label={`Mapped memory by type: ${parts.map((p) => `${REGION_LABEL[p.kind]} ${p.percent.toFixed(0)}%`).join(", ")}`}
        className="flex h-3 overflow-hidden rounded-full bg-surface-2"
      >
        {parts.map((p) => (
          <div
            key={p.kind}
            style={{ width: `${p.percent}%`, background: REGION_COLOR[p.kind] }}
            title={REGION_LABEL[p.kind]}
          />
        ))}
      </div>
      <ul className="flex flex-wrap gap-x-5 gap-y-1">
        {parts.map((p) => (
          <li key={p.kind} className="flex items-center gap-1.5">
            <span
              className="size-2.5 rounded-sm"
              style={{ background: REGION_COLOR[p.kind] }}
              aria-hidden
            />
            {REGION_LABEL[p.kind]} <span className="num text-fg-muted">{formatBytes(p.bytes)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Virtualized: a browser can map tens of thousands of regions. */
function RegionTable({ regions }: { regions: ReturnType<typeof filterRegions> }) {
  const scroller = useRef<HTMLDivElement>(null);
  const rowHeight = scaled(ROW_HEIGHT, useRootPx());
  const virtualizer = useVirtualizer({
    count: regions.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => rowHeight,
    overscan: 12,
  });
  if (regions.length === 0) return <p className="text-fg-muted">No regions match.</p>;
  return (
    <div>
      <div
        className={`grid ${COLUMNS} gap-3 border-b border-border py-1 text-[0.8125rem] font-medium text-fg-muted`}
      >
        <span>Start</span>
        <span>End</span>
        <span>Perms</span>
        <span className="text-right">Size</span>
        <span>Type</span>
        <span>Mapped file</span>
      </div>
      <div ref={scroller} className="max-h-[26rem] overflow-auto">
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((v) => {
            const r = regions[v.index];
            if (!r) return null;
            return (
              <div
                key={v.key}
                className={`num selectable absolute left-0 grid w-full ${COLUMNS} items-center gap-3 border-b border-border/40 font-mono text-[0.8125rem]`}
                style={{ top: v.start, height: rowHeight }}
              >
                <span>{r.start}</span>
                <span>{r.end}</span>
                <span>
                  {r.permissions}
                  {r.shared ? " ⇄" : ""}
                </span>
                <span className="text-right">{formatBytes(r.size)}</span>
                <span className="truncate font-sans">{REGION_LABEL[r.kind]}</span>
                <span className="truncate" title={r.path ?? undefined}>
                  {r.path ?? "—"}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
