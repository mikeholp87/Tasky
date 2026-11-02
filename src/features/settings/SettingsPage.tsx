import { Monitor, Moon, Sun } from "lucide-react";
import { Card, CardHeader } from "@/components/ui/Card";
import { PageHeader } from "@/components/PageHeader";
import { cn } from "@/lib/cn";
import { useSettingsStore, type ThemePreference } from "@/stores/settingsStore";

const THEMES: { id: ThemePreference; label: string; icon: typeof Sun }[] = [
  { id: "system", label: "System", icon: Monitor },
  { id: "light", label: "Light", icon: Sun },
  { id: "dark", label: "Dark", icon: Moon },
];

export function SettingsPage() {
  const theme = useSettingsStore((s) => s.theme);
  const update = useSettingsStore((s) => s.update);

  return (
    <>
      <PageHeader title="Settings" />
      <Card className="max-w-2xl">
        <CardHeader title="Appearance" />
        <div className="px-4 pb-4 pt-3">
          <div id="theme-label" className="mb-2 font-medium">
            Theme
          </div>
          <div
            role="radiogroup"
            aria-labelledby="theme-label"
            className="inline-flex gap-1 rounded-lg bg-surface-2 p-1"
          >
            {THEMES.map(({ id, label, icon: Icon }) => (
              <button
                key={id}
                type="button"
                role="radio"
                aria-checked={theme === id}
                onClick={() => update({ theme: id })}
                className={cn(
                  "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-fg-muted",
                  theme === id && "bg-surface text-fg shadow-sm",
                )}
              >
                <Icon size={14} aria-hidden />
                {label}
              </button>
            ))}
          </div>
        </div>
      </Card>
    </>
  );
}
