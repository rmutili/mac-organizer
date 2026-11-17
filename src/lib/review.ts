// Review state: what will happen to each item. Pure, immutable functions so
// the screens stay simple and this logic is unit-tested.
import type { Decisions, FileItem, Plan } from "./api";

export type Decision =
  | { kind: "move"; folder: string }
  | { kind: "trash" }
  | { kind: "keep" };

export type DecisionMap = Readonly<Record<number, Decision>>;

export type View =
  | { kind: "overview" }
  | { kind: "to-delete" }
  | { kind: "duplicates" }
  | { kind: "attention" }
  | { kind: "unchanged" }
  | { kind: "folder"; name: string };

export interface ViewCount {
  count: number;
  bytes: number;
}

export interface Summary {
  moves: number;
  trash: number;
  freedBytes: number;
}

export function viewKey(view: View): string {
  return view.kind === "folder" ? `folder:${view.name}` : view.kind;
}

/** The move Claude planned for an item, or "keep" if it wasn't placed. */
export function plannedDecision(plan: Plan, itemId: number): Decision {
  const move = plan.moves.find((m) => m.item_id === itemId);
  return move ? { kind: "move", folder: move.folder } : { kind: "keep" };
}

/**
 * Start from Claude's plan. Only confident suggestions are pre-marked for the
 * Trash: high-confidence ones and verified duplicate copies. Uncertain ones
 * stay on their planned move until the person marks them.
 */
export function initialDecisions(items: readonly FileItem[], plan: Plan): DecisionMap {
  const deletions = new Map(plan.deletions.map((d) => [d.item_id, d]));
  return Object.fromEntries(
    items.map((item) => {
      const deletion = deletions.get(item.id);
      const isConfident =
        deletion !== undefined &&
        !item.sensitive &&
        (deletion.confidence === "high" || item.duplicate_group !== null);
      return [item.id, isConfident ? { kind: "trash" } : plannedDecision(plan, item.id)];
    }),
  );
}

export function setDecision(decisions: DecisionMap, itemId: number, decision: Decision): DecisionMap {
  return { ...decisions, [itemId]: decision };
}

/** Mark for the Trash, or unmark back to the planned move. */
export function toggleTrash(decisions: DecisionMap, plan: Plan, itemId: number): DecisionMap {
  const next = decisions[itemId]?.kind === "trash" ? plannedDecision(plan, itemId) : { kind: "trash" as const };
  return setDecision(decisions, itemId, next);
}

export function itemsInView(
  view: View,
  items: readonly FileItem[],
  plan: Plan,
  decisions: DecisionMap,
): FileItem[] {
  switch (view.kind) {
    case "overview":
      return [];
    case "to-delete": {
      const suggested = new Set(plan.deletions.map((d) => d.item_id));
      return items.filter((i) => suggested.has(i.id));
    }
    case "duplicates":
      return items
        .filter((i) => i.duplicate_group !== null)
        .sort((a, b) => (a.duplicate_group ?? 0) - (b.duplicate_group ?? 0) || a.name.localeCompare(b.name));
    case "attention":
      return items.filter((i) => i.sensitive);
    case "unchanged":
      return items.filter((i) => decisions[i.id]?.kind === "keep");
    case "folder":
      return items.filter((i) => {
        const decision = decisions[i.id];
        return decision?.kind === "move" && decision.folder === view.name;
      });
  }
}

export function viewCounts(
  items: readonly FileItem[],
  plan: Plan,
  decisions: DecisionMap,
): Record<string, ViewCount> {
  const views: View[] = [
    { kind: "to-delete" },
    { kind: "duplicates" },
    { kind: "attention" },
    { kind: "unchanged" },
    ...plan.folders.map((f) => ({ kind: "folder" as const, name: f.name })),
  ];
  return Object.fromEntries(
    views.map((view) => {
      const inView = itemsInView(view, items, plan, decisions);
      return [viewKey(view), { count: inView.length, bytes: inView.reduce((sum, i) => sum + i.size_bytes, 0) }];
    }),
  );
}

export function summarize(items: readonly FileItem[], decisions: DecisionMap): Summary {
  return items.reduce<Summary>(
    (acc, item) => {
      const kind = decisions[item.id]?.kind;
      if (kind === "move") return { ...acc, moves: acc.moves + 1 };
      if (kind === "trash") return { ...acc, trash: acc.trash + 1, freedBytes: acc.freedBytes + item.size_bytes };
      return acc;
    },
    { moves: 0, trash: 0, freedBytes: 0 },
  );
}

/** The payload for apply_plan: IDs and folder names only. */
export function toDecisions(decisions: DecisionMap): Decisions {
  const entries = Object.entries(decisions)
    .map(([id, decision]) => [Number(id), decision] as const)
    .sort(([a], [b]) => a - b);
  return {
    moves: entries.flatMap(([item_id, d]) => (d.kind === "move" ? [{ item_id, folder: d.folder }] : [])),
    trash: entries.flatMap(([item_id, d]) => (d.kind === "trash" ? [item_id] : [])),
  };
}

/** How many verified duplicate groups would lose every copy. */
export function fullyTrashedDuplicateGroups(items: readonly FileItem[], decisions: DecisionMap): number {
  const groups = new Map<number, boolean>();
  for (const item of items) {
    if (item.duplicate_group === null) continue;
    const isTrashed = decisions[item.id]?.kind === "trash";
    groups.set(item.duplicate_group, (groups.get(item.duplicate_group) ?? true) && isTrashed);
  }
  return [...groups.values()].filter(Boolean).length;
}
