import type { StartupItem } from "@/types";

/**
 * Items matching the search in name, command or description, by name. The
 * order ignores enabled state so a row stays put when its switch is flipped.
 */
export function filterItems(items: readonly StartupItem[], query: string): StartupItem[] {
  const q = query.trim().toLowerCase();
  return items
    .filter(
      (i) =>
        !q || [i.name, i.command, i.comment ?? ""].some((text) => text.toLowerCase().includes(q)),
    )
    .sort((a, b) => a.name.localeCompare(b.name));
}
