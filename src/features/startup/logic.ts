import type { StartupItem } from "@/types";

/** Items matching the search in name, command or description; enabled first. */
export function filterItems(items: readonly StartupItem[], query: string): StartupItem[] {
  const q = query.trim().toLowerCase();
  return items
    .filter(
      (i) =>
        !q || [i.name, i.command, i.comment ?? ""].some((text) => text.toLowerCase().includes(q)),
    )
    .sort((a, b) => Number(b.enabled) - Number(a.enabled) || a.name.localeCompare(b.name));
}
