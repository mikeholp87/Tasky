import { memo } from "react";
import { cn } from "@/lib/cn";
import { useUiStore } from "@/stores/uiStore";
import { ROUTES, type RouteDef } from "./routes";

const GROUP_LABELS: Record<RouteDef["group"], string | null> = {
  main: null,
  resources: "Resources",
  tools: "Tools",
};

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
      className="flex w-52 shrink-0 flex-col border-r border-border bg-surface"
    >
      <div className="flex h-12 items-center gap-2 px-4">
        <span
          aria-hidden
          className="grid size-6 place-items-center rounded-md bg-accent text-[11px] font-bold text-accent-fg"
        >
          TM
        </span>
        <span className="font-semibold">Task Manager</span>
      </div>
      <div className="flex-1 overflow-y-auto px-2 pb-3">
        {groups.map((group) => (
          <div key={group.id} className="mt-2">
            {GROUP_LABELS[group.id] && (
              <div className="px-2 pb-1 pt-2 text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
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
                        "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left text-fg-muted transition-colors",
                        "hover:bg-surface-2 hover:text-fg",
                        active && "bg-accent-soft font-medium text-fg hover:bg-accent-soft",
                      )}
                    >
                      <Icon
                        size={16}
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
