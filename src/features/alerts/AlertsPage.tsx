import { Bell, BellRing, Pencil, Plus, ShieldCheck, Trash2 } from "lucide-react";
import { useState } from "react";
import { PageHeader } from "@/components/PageHeader";
import { Card, CardHeader } from "@/components/ui/Card";
import { Switch } from "@/components/ui/Switch";
import { useAlertStore } from "@/stores/alertStore";
import { useSettingsStore } from "@/stores/settingsStore";
import type { AlertRule } from "@/types";
import { formatTimestamp } from "@/utils/format";
import {
  METRICS,
  describeRule,
  displayThreshold,
  finalizeRule,
  isProcessMetric,
  metricInfo,
  newRule,
  ruleProblem,
  storedThreshold,
} from "./rules";

export function AlertsPage() {
  const rules = useSettingsStore((s) => s.alertRules);
  const notifications = useSettingsStore((s) => s.notificationsEnabled);
  const update = useSettingsStore((s) => s.update);
  const events = useAlertStore((s) => s.events);
  const replaceEvents = useAlertStore((s) => s.replace);
  const [editing, setEditing] = useState<AlertRule | null>(null);

  const enabledCount = rules.filter((r) => r.enabled).length;

  const save = (rule: AlertRule) => {
    const exists = rules.some((r) => r.id === rule.id);
    update({
      alertRules: exists ? rules.map((r) => (r.id === rule.id ? rule : r)) : [...rules, rule],
    });
    setEditing(null);
  };

  return (
    <>
      <PageHeader
        title="Alerts"
        description="Get notified when resources stay above a threshold. Alerts are checked in the background, even while the window is in the tray."
        actions={
          <button
            type="button"
            onClick={() => setEditing(newRule())}
            className="flex h-8 items-center gap-1.5 rounded-md bg-accent px-3 font-medium text-accent-fg"
          >
            <Plus size={14} aria-hidden /> New alert
          </button>
        }
      />

      <div className="space-y-4">
        <div className="grid gap-4 sm:grid-cols-3">
          <Card>
            <CardHeader title="Active Rules" icon={<ShieldCheck size={14} aria-hidden />} />
            <div className="space-y-1.5 px-4 pb-4 pt-3">
              <div className="num text-xl font-semibold">
                {enabledCount} / {rules.length}
              </div>
              <div className="text-[12px] text-fg-muted">
                {enabledCount === 0
                  ? "No rules currently monitoring"
                  : `${enabledCount} ${enabledCount === 1 ? "rule" : "rules"} actively evaluated`}
              </div>
            </div>
          </Card>

          <Card>
            <CardHeader
              title="Desktop Notifications"
              icon={<Bell size={14} aria-hidden />}
              action={
                <Switch
                  label="Desktop notifications"
                  checked={notifications}
                  onChange={(enabled) => update({ notificationsEnabled: enabled })}
                />
              }
            />
            <div className="space-y-1.5 px-4 pb-4 pt-3">
              <div className="text-xl font-semibold">
                {notifications ? "Enabled" : "Disabled"}
              </div>
              <div className="text-[12px] text-fg-muted">
                {notifications
                  ? "System banners shown when thresholds fire"
                  : "Silent mode — events logged in list only"}
              </div>
            </div>
          </Card>

          <Card>
            <CardHeader
              title="Fired Alert Events"
              icon={<BellRing size={14} aria-hidden />}
              action={
                events.length > 0 ? (
                  <button
                    type="button"
                    onClick={() => replaceEvents([])}
                    className="rounded-md border border-border px-2 py-0.5 text-[11px] font-medium text-fg-muted hover:bg-surface-2 hover:text-fg"
                  >
                    Clear history
                  </button>
                ) : null
              }
            />
            <div className="space-y-1.5 px-4 pb-4 pt-3">
              <div className="num text-xl font-semibold">{events.length}</div>
              <div className="truncate text-[12px] text-fg-muted">
                {events[0]
                  ? `Last: ${events[0].ruleName} (${formatTimestamp(events[0].timestampMs)})`
                  : "No alerts triggered in this session"}
              </div>
            </div>
          </Card>
        </div>

        <div className="grid gap-4 xl:grid-cols-[1fr_420px]">
          <Card>
            <CardHeader title={`Rules (${rules.length})`} />
            <ul className="divide-y divide-border px-4 pb-2">
              {rules.length === 0 && <li className="py-6 text-fg-muted">No alert rules yet.</li>}
              {rules.map((rule) =>
                editing?.id === rule.id ? (
                  <li key={rule.id} className="py-3">
                    <RuleEditor rule={editing} onCancel={() => setEditing(null)} onSave={save} />
                  </li>
                ) : (
                  <li key={rule.id} className="flex items-center gap-3 py-2">
                    <div className="min-w-0 flex-1">
                      <div className="truncate font-medium">{rule.name}</div>
                      <div className="truncate text-fg-muted">{describeRule(rule)}</div>
                    </div>
                    <div>
                      <Switch
                        label={`Enable ${rule.name}`}
                        checked={rule.enabled}
                        onChange={(enabled) =>
                          update({
                            alertRules: rules.map((r) => (r.id === rule.id ? { ...r, enabled } : r)),
                          })
                        }
                      />
                    </div>
                    <button
                      type="button"
                      aria-label={`Edit ${rule.name}`}
                      onClick={() => setEditing(rule)}
                      className="grid size-8 place-items-center rounded text-fg-muted hover:bg-surface-2 hover:text-fg"
                    >
                      <Pencil size={14} />
                    </button>
                    <button
                      type="button"
                      aria-label={`Delete ${rule.name}`}
                      onClick={() => update({ alertRules: rules.filter((r) => r.id !== rule.id) })}
                      className="grid size-8 place-items-center rounded text-fg-muted hover:bg-surface-2 hover:text-crit"
                    >
                      <Trash2 size={14} />
                    </button>
                  </li>
                ),
              )}
              {editing && !rules.some((r) => r.id === editing.id) && (
                <li className="py-3">
                  <RuleEditor rule={editing} onCancel={() => setEditing(null)} onSave={save} />
                </li>
              )}
            </ul>
          </Card>

          <Card>
            <CardHeader
              title="Recent alerts"
              icon={<BellRing size={14} aria-hidden />}
              action={
                events.length > 0 ? (
                  <button
                    type="button"
                    onClick={() => replaceEvents([])}
                    className="rounded-md border border-border px-2 py-0.5 text-[11px] font-medium text-fg-muted hover:bg-surface-2 hover:text-fg"
                  >
                    Clear history
                  </button>
                ) : null
              }
            />
            <ol className="max-h-[60vh] space-y-2 overflow-auto px-4 pb-4 pt-3">
              {events.length === 0 && <li className="text-fg-muted">No alerts have fired.</li>}
              {events.map((e, i) => (
                <li key={`${e.timestampMs}-${e.ruleId}-${i}`} className="rounded-md bg-surface-2 p-2">
                  <div className="flex justify-between gap-2 text-[12px] text-fg-muted">
                    <span className="truncate">{e.ruleName}</span>
                    <time className="num shrink-0">{formatTimestamp(e.timestampMs)}</time>
                  </div>
                  <p>{e.message}</p>
                </li>
              ))}
            </ol>
          </Card>
        </div>
      </div>
    </>
  );
}

const inputCls =
  "h-8 w-full rounded-md border border-border bg-surface px-2 outline-none focus:border-accent";

function RuleEditor({
  rule: initial,
  onSave,
  onCancel,
}: {
  rule: AlertRule;
  onSave: (rule: AlertRule) => void;
  onCancel: () => void;
}) {
  const [rule, setRule] = useState(initial);
  const [threshold, setThreshold] = useState(String(displayThreshold(initial)));
  const draft: AlertRule = { ...rule, threshold: storedThreshold(rule.metric, Number(threshold)) };
  const problem = ruleProblem(draft);
  const unit = metricInfo(rule.metric).unit;

  return (
    <form
      className="grid gap-3 rounded-md border border-border p-3 sm:grid-cols-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (!problem) onSave(finalizeRule(draft));
      }}
    >
      <label className="sm:col-span-2">
        <span className="mb-1 block text-fg-muted">Name</span>
        <input
          className={inputCls}
          value={rule.name}
          maxLength={100}
          onChange={(e) => setRule({ ...rule, name: e.target.value })}
          placeholder="e.g. Node using too much CPU"
          autoFocus
        />
      </label>
      <label>
        <span className="mb-1 block text-fg-muted">When</span>
        <select
          className={inputCls}
          value={rule.metric}
          onChange={(e) => {
            const metric = e.target.value as AlertRule["metric"];
            setRule({ ...rule, metric });
            if (metric === "processMemory") setThreshold("4");
            else if (rule.metric === "processMemory") setThreshold("80");
          }}
        >
          {METRICS.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
      </label>
      {isProcessMetric(rule.metric) ? (
        <label>
          <span className="mb-1 block text-fg-muted">Process name</span>
          <input
            className={inputCls}
            value={rule.processName ?? ""}
            onChange={(e) => setRule({ ...rule, processName: e.target.value })}
            placeholder="e.g. node"
          />
        </label>
      ) : (
        <div />
      )}
      <label>
        <span className="mb-1 block text-fg-muted">Is above ({unit})</span>
        <input
          className={inputCls}
          type="number"
          min={0}
          step="any"
          value={threshold}
          onChange={(e) => setThreshold(e.target.value)}
        />
      </label>
      <label>
        <span className="mb-1 block text-fg-muted">For (seconds)</span>
        <input
          className={inputCls}
          type="number"
          min={0}
          max={3600}
          step={1}
          value={rule.durationSecs}
          onChange={(e) => setRule({ ...rule, durationSecs: Math.trunc(Number(e.target.value)) })}
        />
      </label>
      <div className="flex items-center justify-between gap-2 sm:col-span-2">
        <span role="status" className="text-warn">
          {problem && rule.name ? problem : ""}
        </span>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="h-8 rounded-md border border-border px-3"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={problem != null}
            className="h-8 rounded-md bg-accent px-3 font-medium text-accent-fg disabled:opacity-50"
          >
            Save alert
          </button>
        </div>
      </div>
    </form>
  );
}
