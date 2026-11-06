/**
 * Process tree construction. Pure functions, unit-tested.
 *
 * Parent PIDs can be stale: when a parent exits, its PID may be reused by an
 * unrelated, newer process. A process is therefore only attached to a parent
 * that started no later than itself; otherwise it becomes a root. This also
 * makes cycles impossible except for identical start times, which the
 * traversal guards against separately.
 */
import type { ProcessRow } from "@/types";

export interface TableItem {
  row: ProcessRow;
  /** Present in tree mode only. */
  depth?: number;
  hasChildren?: boolean;
  expanded?: boolean;
  /** Applications view: a synthetic row summing the processes of one program. */
  group?: boolean;
}

export interface Forest {
  roots: ProcessRow[];
  children: Map<number, ProcessRow[]>;
}

export function isPlausibleParent(parent: ProcessRow, child: ProcessRow): boolean {
  if (parent.pid === child.pid) return false;
  // Unknown start times (0) cannot disprove the relationship.
  if (parent.startTime > 0 && child.startTime > 0 && parent.startTime > child.startTime) {
    return false;
  }
  return true;
}

export function buildForest(rows: readonly ProcessRow[]): Forest {
  const byPid = new Map<number, ProcessRow>();
  for (const r of rows) byPid.set(r.pid, r);
  const roots: ProcessRow[] = [];
  const children = new Map<number, ProcessRow[]>();
  for (const r of rows) {
    const parent = r.parentPid != null ? byPid.get(r.parentPid) : undefined;
    if (parent && isPlausibleParent(parent, r)) {
      const list = children.get(parent.pid);
      if (list) list.push(r);
      else children.set(parent.pid, [r]);
    } else {
      roots.push(r);
    }
  }

  // Processes in a parent cycle are unreachable from every root. Never hide a
  // process: promote one member of each unreachable group to a root.
  const reached = new Set<number>();
  const mark = (start: ProcessRow) => {
    const stack = [start];
    while (stack.length > 0) {
      const r = stack.pop() as ProcessRow;
      if (reached.has(r.pid)) continue;
      reached.add(r.pid);
      for (const c of children.get(r.pid) ?? []) stack.push(c);
    }
  };
  roots.forEach(mark);
  for (const r of rows) {
    if (!reached.has(r.pid)) {
      roots.push(r);
      mark(r);
    }
  }
  return { roots, children };
}

export interface FlattenOptions {
  /** PIDs whose children are hidden. Everything else is expanded. */
  collapsed: ReadonlySet<number>;
  compare: (a: ProcessRow, b: ProcessRow) => number;
  /**
   * When set, only matching processes and their ancestors are included.
   * Collapsed ancestors still hide their descendants.
   */
  matches?: (row: ProcessRow) => boolean;
}

/** Depth-first, sibling-sorted list of visible tree rows. */
export function flattenTree(rows: readonly ProcessRow[], options: FlattenOptions): TableItem[] {
  const { roots, children } = buildForest(rows);
  const { collapsed, compare, matches } = options;

  // When filtering, keep a node if it or any descendant matches.
  let keep: Set<number> | null = null;
  if (matches) {
    keep = new Set();
    const parent = new Map<number, number>();
    for (const [pid, kids] of children) for (const child of kids) parent.set(child.pid, pid);
    const pending = rows.filter(matches).map((row) => row.pid);
    while (pending.length > 0) {
      const pid = pending.pop()!;
      if (keep.has(pid)) continue;
      keep.add(pid);
      const ancestor = parent.get(pid);
      if (ancestor !== undefined) pending.push(ancestor);
    }
  }

  const out: TableItem[] = [];
  const visited = new Set<number>();
  // Explicit stack avoids a call-stack overflow on deeply nested processes.
  const pending = [...roots]
    .sort(compare)
    .reverse()
    .map((row) => ({ row, depth: 0 }));
  while (pending.length > 0) {
    const { row, depth } = pending.pop()!;
    if (visited.has(row.pid)) continue;
    if (keep && !keep.has(row.pid)) continue;
    visited.add(row.pid);
    const kids = (children.get(row.pid) ?? []).filter((c) => !keep || keep.has(c.pid));
    const hasChildren = kids.length > 0;
    const expanded = hasChildren && !collapsed.has(row.pid);
    out.push({ row, depth, hasChildren, expanded });
    if (expanded) {
      for (const child of kids.sort(compare).reverse())
        pending.push({ row: child, depth: depth + 1 });
    }
  }
  return out;
}

/** PIDs that have at least one child, for "Collapse all". */
export function parentPids(rows: readonly ProcessRow[]): Set<number> {
  return new Set(buildForest(rows).children.keys());
}

/** Direct children of `pid` (validated against PID reuse). */
export function childrenOf(rows: readonly ProcessRow[], pid: number): ProcessRow[] {
  return buildForest(rows).children.get(pid) ?? [];
}
