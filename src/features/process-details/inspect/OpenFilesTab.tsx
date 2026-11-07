import { useMemo, useState } from "react";
import { SearchBox } from "@/components/SearchBox";
import { useRemoteData } from "@/hooks/useRemoteData";
import { backend } from "@/services/ipc";
import { ACCESS_LABEL, FILE_KIND_LABEL, filterOpenFiles } from "./logic";
import { InspectBody } from "./InspectBody";

/** Loaded once when the tab opens (descriptor listing can be large); Refresh re-reads. */
export function OpenFilesTab({ pid, startTime }: { pid: number; startTime: number }) {
  const [query, setQuery] = useState("");
  const { data, error, loading, refresh } = useRemoteData(
    () => backend.getOpenFiles(pid, startTime),
    `files:${pid}:${startTime}`,
  );
  const shown = useMemo(() => filterOpenFiles(data?.files ?? [], query), [data, query]);
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
            label="Search open files"
            placeholder="Path, descriptor, type…"
          />
          {data && (
            <span className="text-fg-muted">
              {shown.length} of {data.files.length}
            </span>
          )}
        </>
      }
      note={
        data?.truncated
          ? "Only the first entries are shown; this process holds more descriptors than the list limit."
          : undefined
      }
    >
      {shown.length === 0 ? (
        <p className="text-fg-muted">{data?.files.length ? "No matches." : "No open files."}</p>
      ) : (
        <div className="max-h-[28rem] overflow-auto">
          <table className="w-full text-left text-[0.8125rem]">
            <thead className="sticky top-0 bg-surface text-fg-muted">
              <tr>
                <th className="py-1 pr-3 font-medium">FD</th>
                <th className="py-1 pr-3 font-medium">Type</th>
                <th className="py-1 pr-3 font-medium">Access</th>
                <th className="py-1 font-medium">Target</th>
              </tr>
            </thead>
            <tbody className="num selectable">
              {shown.map((f) => (
                <tr key={f.descriptor} className="border-t border-border/60 align-top">
                  <td className="py-1 pr-3 font-mono">{f.descriptor}</td>
                  <td className="py-1 pr-3">{FILE_KIND_LABEL[f.kind]}</td>
                  <td className="py-1 pr-3">{f.access ? ACCESS_LABEL[f.access] : "—"}</td>
                  <td className="break-all py-1 font-mono">{f.target}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </InspectBody>
  );
}
