import { Card, CardHeader } from "@/components/ui/Card";
import { STATE_LABEL, endpoint } from "@/features/ports/logic";
import type { Connection } from "@/types";
import { NOT_AVAILABLE } from "@/utils/format";

export function ConnectionsCard({
  connections,
  loading,
}: {
  connections: Connection[] | null | undefined;
  loading: boolean;
}) {
  return (
    <Card>
      <CardHeader
        title={connections ? `Network connections (${connections.length})` : "Network connections"}
      />
      <div className="max-h-72 overflow-auto px-4 pb-4 pt-2">
        {connections === undefined ? (
          <p className="text-fg-muted">{loading ? "Loading…" : NOT_AVAILABLE}</p>
        ) : connections === null ? (
          <p className="text-fg-muted">
            Unavailable on this platform or for this process. Listing another account's sockets may
            require administrator/root rights.
          </p>
        ) : connections.length === 0 ? (
          <p className="text-fg-muted">No open TCP or UDP sockets.</p>
        ) : (
          <table className="w-full text-left text-[0.8125rem]">
            <thead className="text-fg-muted">
              <tr>
                <th className="py-1 pr-3 font-medium">Protocol</th>
                <th className="py-1 pr-3 font-medium">Local</th>
                <th className="py-1 pr-3 font-medium">Remote</th>
                <th className="py-1 font-medium">State</th>
              </tr>
            </thead>
            <tbody className="num selectable font-mono">
              {connections.map((c, i) => (
                <tr key={i} className="border-t border-border/60">
                  <td className="py-1 pr-3 uppercase">{c.protocol}</td>
                  <td className="py-1 pr-3">{endpoint(c.localAddress, c.localPort)}</td>
                  <td className="py-1 pr-3">
                    {c.remoteAddress != null ? endpoint(c.remoteAddress, c.remotePort) : "—"}
                  </td>
                  <td className="py-1 font-sans">{c.state ? STATE_LABEL[c.state] : "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </Card>
  );
}
