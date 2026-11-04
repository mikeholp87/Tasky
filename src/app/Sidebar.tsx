import { memo } from "react";
import { cn } from "@/lib/cn";
import { useUiStore } from "@/stores/uiStore";
import { ROUTES, type RouteDef } from "./routes";

const GROUP_LABELS: Record<RouteDef["group"], string | null> = {
  main: null,
  resources: "Resources",
  tools: "Tools",
};

/** Four load bars: the same motif as the per-core strip. */
function LogoMark() {
  return (
    <svg aria-hidden viewBox="0 0 24 24" className="size-7 shrink-0">
      <rect x="3" y="12" width="3.5" height="9" rx="1" fill="var(--res-net)" />
      <rect x="8.5" y="6" width="3.5" height="15" rx="1" fill="var(--res-cpu)" />
      <rect x="14" y="9" width="3.5" height="12" rx="1" fill="var(--res-mem)" />
      <rect x="19.5" y="3" width="3.5" height="18" rx="1" fill="var(--accent)" />
    </svg>
  );
}

export const Sidebar = memo(function Sidebar() {
  const route = useUiStore((s) => s.route);
  const navigate = useUiStore((s) => s.navigate);
  const groups = (["main", "resources", "tools"] as const).map((g) => ({
    id: g,
    items: ROUTES.filter((r) => r.group === g),
  }));

  return (
    <nav
      aria-label="Primary"
      className="flex w-60 shrink-0 flex-col border-r border-border bg-surface"
    >
      <div className="flex h-14 items-center gap-2.5 px-4">
        <LogoMark />
        <span className="text-[1.0625rem] font-semibold tracking-tight">Task Manager</span>
      </div>
      <div className="flex-1 overflow-y-auto px-2 pb-3">
        {groups.map((group) => (
          <div key={group.id} className="mt-2">
            {GROUP_LABELS[group.id] && (
              <div className="px-3 pb-1.5 pt-4 text-[0.8125rem] font-medium text-fg-subtle">
                {GROUP_LABELS[group.id]}
              </div>
            )}
            <ul className="space-y-px">
              {group.items.map((item) => {
                const Icon = item.icon;
                const active = item.id === route;
                return (
                  <li key={item.id}>
                    <button
                      type="button"
                      onClick={() => navigate(item.id)}
                      aria-current={active ? "page" : undefined}
                      className={cn(
                        "relative flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[0.9375rem] text-fg-muted transition-colors",
                        "hover:bg-surface-2 hover:text-fg",
                        active &&
                          "bg-surface-2 font-medium text-fg before:absolute before:inset-y-1.5 before:left-0 before:w-[3px] before:rounded-full before:bg-accent",
                      )}
                    >
                      <Icon
                        size="1.25em"
                        strokeWidth={1.75}
                        className={active ? "text-accent" : undefined}
                        aria-hidden
                      />
                      {item.label}
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </div>
    </nav>
  );
});
