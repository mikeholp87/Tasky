import { useMemo, useState } from "react";
import { SearchBox } from "@/components/SearchBox";
import { useRemoteData } from "@/hooks/useRemoteData";
import { backend } from "@/services/ipc";
import { formatBytes } from "@/utils/format";
import { filterModules } from "./logic";
import { InspectBody } from "./InspectBody";

export function ModulesTab({ pid, windows }: { pid: number; windows: boolean }) {
  const [query, setQuery] = useState("");
  const { data, error, loading, refresh } = useRemoteData(
    () => backend.getModules(pid),
    `modules:${pid}`,
  );
  const shown = useMemo(() => filterModules(data ?? [], query), [data, query]);
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
            label="Search modules"
            placeholder="Name or path…"
          />
          {data && (
            <span className="text-fg-muted">
              {shown.length} of {data.length}
            </span>
          )}
        </>
      }
      note={
        windows
          ? undefined
          : "The program and shared libraries mapped into this process. Data files and shared memory are in the Memory Map tab."
      }
    >
      {shown.length === 0 ? (
        <p className="text-fg-muted">{data?.length ? "No matches." : "No modules."}</p>
      ) : (
        <div className="max-h-[28rem] overflow-auto">
          <table className="w-full text-left text-[0.8125rem]">
            <thead className="sticky top-0 bg-surface text-fg-muted">
              <tr>
                <th className="py-1 pr-3 font-medium">Module</th>
                <th className="py-1 pr-3 font-medium">Base address</th>
                <th className="py-1 pr-3 font-medium">{windows ? "Size" : "Mapped size"}</th>
                <th className="py-1 font-medium">Path</th>
              </tr>
            </thead>
            <tbody className="num selectable">
              {shown.map((m) => (
                <tr key={`${m.base}:${m.path}`} className="border-t border-border/60 align-top">
                  <td className="py-1 pr-3 font-medium">{m.name}</td>
                  <td className="py-1 pr-3 font-mono">0x{m.base}</td>
                  <td className="py-1 pr-3">{formatBytes(m.size)}</td>
                  <td className="break-all py-1 font-mono">{m.path}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </InspectBody>
  );
}
