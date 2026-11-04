import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
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
  const collapsed = useUiStore((s) => s.sidebarCollapsed);
  const toggleSidebar = useUiStore((s) => s.toggleSidebar);
  const groups = (["main", "resources", "tools"] as const).map((g) => ({
    id: g,
    items: ROUTES.filter((r) => r.group === g),
  }));

  return (
    <nav
      aria-label="Primary"
      className={cn(
        "flex shrink-0 flex-col border-r border-border bg-surface transition-[width] duration-150",
        collapsed ? "w-14" : "w-52",
      )}
    >
      <div
        className={cn(
          "flex h-12 items-center border-b border-border/60",
          collapsed ? "justify-center px-2" : "justify-between gap-2 px-3",
        )}
      >
        <button
          type="button"
          onClick={toggleSidebar}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          className="flex items-center gap-2 rounded-md p-1 text-left transition-colors hover:bg-surface-2"
        >
          <span
            aria-hidden
            className="grid size-6 shrink-0 place-items-center rounded-md bg-accent text-[11px] font-bold text-accent-fg"
          >
            TM
          </span>
          {!collapsed && <span className="truncate font-semibold">Task Manager</span>}
        </button>
        {!collapsed && (
          <button
            type="button"
            onClick={toggleSidebar}
            title="Collapse sidebar"
            aria-label="Collapse sidebar"
            className="grid size-7 place-items-center rounded-md text-fg-muted transition-colors hover:bg-surface-2 hover:text-fg"
          >
            <PanelLeftClose size={15} />
          </button>
        )}
      </div>
      <div className="flex-1 overflow-y-auto px-2 pb-3">
        {groups.map((group) => (
          <div key={group.id} className="mt-2">
            {GROUP_LABELS[group.id] &&
              (collapsed ? (
                <div className="mx-auto my-2 h-px w-6 bg-border" aria-hidden />
              ) : (
                <div className="px-2 pb-1 pt-2 text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
                  {GROUP_LABELS[group.id]}
                </div>
              ))}
            <ul className="space-y-0.5">
              {group.items.map((item) => {
                const Icon = item.icon;
                const active = item.id === route;
                return (
                  <li key={item.id}>
                    <button
                      type="button"
                      onClick={() => navigate(item.id)}
                      aria-current={active ? "page" : undefined}
                      title={collapsed ? item.label : undefined}
                      className={cn(
                        "flex w-full items-center rounded-md py-1.5 text-left text-fg-muted transition-colors",
                        collapsed ? "justify-center px-0" : "gap-2.5 px-2",
                        "hover:bg-surface-2 hover:text-fg",
                        active && "bg-accent-soft font-medium text-fg hover:bg-accent-soft",
                      )}
                    >
                      <Icon
                        size={16}
                        strokeWidth={1.75}
                        className={cn("shrink-0", active && "text-accent")}
                        aria-hidden
                      />
                      <span className={cn(collapsed ? "sr-only" : "truncate")}>{item.label}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </div>
      <div className="border-t border-border p-2">
        <button
          type="button"
          onClick={toggleSidebar}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          className={cn(
            "flex w-full items-center rounded-md py-1.5 text-fg-muted transition-colors hover:bg-surface-2 hover:text-fg",
            collapsed ? "justify-center px-0" : "gap-2.5 px-2",
          )}
        >
          {collapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
          {!collapsed && <span className="truncate text-[12px]">Collapse sidebar</span>}
        </button>
      </div>
    </nav>
  );
});
