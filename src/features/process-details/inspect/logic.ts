import type {
  FileAccess,
  MemoryKindTotal,
  MemoryRegion,
  MemoryRegionKind,
  ModuleInfo,
  OpenFile,
  OpenFileKind,
} from "@/types";

export const REGION_LABEL: Record<MemoryRegionKind, string> = {
  image: "Program image",
  sharedLibrary: "Shared libraries",
  mappedFile: "Mapped files",
  heap: "Heap",
  stack: "Main stack",
  special: "Kernel-provided",
  anonymous: "Anonymous memory",
};

/** Theme colors; neutral for kinds that carry no emphasis. */
export const REGION_COLOR: Record<MemoryRegionKind, string> = {
  image: "var(--res-cpu)",
  sharedLibrary: "var(--res-net)",
  mappedFile: "var(--res-disk)",
  heap: "var(--res-mem)",
  stack: "var(--res-disk-2)",
  special: "var(--fg-subtle)",
  anonymous: "var(--border-strong)",
};

export const FILE_KIND_LABEL: Record<OpenFileKind, string> = {
  file: "File",
  directory: "Directory",
  socket: "Socket",
  pipe: "Pipe",
  device: "Device",
  anonymous: "Kernel object",
  other: "Other",
};

export const ACCESS_LABEL: Record<FileAccess, string> = {
  read: "Read",
  write: "Write",
  readWrite: "Read/write",
};

export interface Segment {
  kind: MemoryRegionKind;
  bytes: number;
  regions: number;
  percent: number;
}

/** Shares of the mapped address space for the summary bar. */
export function segments(summary: readonly MemoryKindTotal[]): Segment[] {
  const total = summary.reduce((sum, s) => sum + s.bytes, 0);
  if (total <= 0) return [];
  return summary
    .filter((s) => s.bytes > 0)
    .map((s) => ({ ...s, percent: (s.bytes / total) * 100 }));
}

const has = (text: string | null | undefined, q: string) =>
  text != null && text.toLowerCase().includes(q);

export function filterRegions(
  regions: readonly MemoryRegion[],
  kind: MemoryRegionKind | "all",
  query: string,
): MemoryRegion[] {
  const q = query.trim().toLowerCase();
  return regions.filter(
    (r) =>
      (kind === "all" || r.kind === kind) &&
      (!q || has(r.path, q) || r.start.includes(q) || r.end.includes(q) || r.permissions === q),
  );
}

export function filterOpenFiles(files: readonly OpenFile[], query: string): OpenFile[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...files];
  return files.filter(
    (f) =>
      has(f.target, q) || f.descriptor === q || FILE_KIND_LABEL[f.kind].toLowerCase().includes(q),
  );
}

export function filterModules(modules: readonly ModuleInfo[], query: string): ModuleInfo[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...modules];
  return modules.filter((m) => has(m.name, q) || has(m.path, q));
}
