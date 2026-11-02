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
   * When set, only matching processes and their ancestors are shown, and
   * ancestors of matches are expanded regardless of `collapsed`.
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
    const visit = (row: ProcessRow, seen: Set<number>): boolean => {
      if (seen.has(row.pid)) return false;
      seen.add(row.pid);
      let any = matches(row);
      for (const child of children.get(row.pid) ?? []) {
        if (visit(child, seen)) any = true;
      }
      if (any) keep?.add(row.pid);
      return any;
    };
    const seen = new Set<number>();
    for (const root of roots) visit(root, seen);
  }

  const out: TableItem[] = [];
  const visited = new Set<number>();
  const walk = (list: ProcessRow[], depth: number) => {
    const sorted = [...list].sort(compare);
    for (const row of sorted) {
      if (visited.has(row.pid)) continue;
      if (keep && !keep.has(row.pid)) continue;
      visited.add(row.pid);
      const kids = (children.get(row.pid) ?? []).filter((c) => !keep || keep.has(c.pid));
      const hasChildren = kids.length > 0;
      // While filtering, always reveal the path to matches.
      const expanded = hasChildren && (keep !== null || !collapsed.has(row.pid));
      out.push({ row, depth, hasChildren, expanded });
      if (expanded) walk(kids, depth + 1);
    }
  };
  walk(roots, 0);
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
