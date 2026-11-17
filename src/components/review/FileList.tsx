import { useEffect, useRef } from "react";
import type { FileItem, Plan } from "../../lib/api";
import { formatAge, formatBytes, nowSeconds } from "../../lib/format";
import type { Decision, DecisionMap } from "../../lib/review";
import { DecisionControl } from "./DecisionControl";

export type SortKey = "size" | "name" | "age";

interface FileListProps {
  title: string;
  subtitle: string;
  items: FileItem[];
  plan: Plan;
  decisions: DecisionMap;
  selectedId: number | null;
  sort: SortKey;
  onSort: (sort: SortKey) => void;
  onSelect: (id: number) => void;
  onDecide: (id: number, decision: Decision) => void;
  onToggleTrash: (id: number) => void;
  onQuickLook: (id: number) => void;
}

export function sortItems(items: FileItem[], sort: SortKey): FileItem[] {
  const sorted = [...items];
  if (sort === "size") return sorted.sort((a, b) => b.size_bytes - a.size_bytes);
  if (sort === "age") return sorted.sort((a, b) => a.modified_at - b.modified_at);
  return sorted.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
}

const SORTS: { key: SortKey; label: string }[] = [
  { key: "name", label: "Name" },
  { key: "size", label: "Size" },
  { key: "age", label: "Modified" },
];

export function FileList(props: FileListProps) {
  const { items, plan, decisions, selectedId, onSelect } = props;
  const rowsRef = useRef<HTMLDivElement>(null);
  const now = nowSeconds();
  const largest = Math.max(1, ...items.map((i) => i.size_bytes));
  const deletions = new Map(plan.deletions.map((d) => [d.item_id, d]));
  const planned = new Map(plan.moves.map((m) => [m.item_id, m.folder]));

  useEffect(() => {
    rowsRef.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [selectedId]);

  function onKeyDown(event: React.KeyboardEvent) {
    const index = items.findIndex((i) => i.id === selectedId);
    const move = (delta: number) => {
      const next = items[Math.min(items.length - 1, Math.max(0, index + delta))];
      if (next) onSelect(next.id);
    };
    if (event.key === "ArrowDown") move(1);
    else if (event.key === "ArrowUp") move(-1);
    else if (selectedId === null) return;
    else if (event.key === "Backspace" || event.key === "Delete") props.onToggleTrash(selectedId);
    else if (event.key === "k") props.onDecide(selectedId, { kind: "keep" });
    else if (event.key === " ") props.onQuickLook(selectedId);
    else return;
    event.preventDefault();
  }

  return (
    <section className="list" aria-labelledby="list-title">
      <header className="list__header">
        <h2 id="list-title" className="list__title">{props.title}</h2>
        <p className="list__subtitle">{props.subtitle}</p>
      </header>

      <div className="list__columns" role="row">
        {SORTS.map((s) => (
          <button key={s.key} className="sort-button" aria-pressed={props.sort === s.key} onClick={() => props.onSort(s.key)}>
            {s.label}
          </button>
        ))}
        <span>Decision</span>
      </div>

      <div
        ref={rowsRef}
        className="list__rows"
        role="listbox"
        aria-label={props.title}
        tabIndex={0}
        onKeyDown={onKeyDown}
      >
        {items.length === 0 && <p className="empty-list">Nothing here.</p>}
        {items.map((item) => {
          const decision = decisions[item.id] ?? { kind: "keep" };
          const deletion = deletions.get(item.id);
          return (
            <div
              key={item.id}
              className={`row${decision.kind === "trash" ? " row--trash" : ""}`}
              role="option"
              aria-selected={item.id === selectedId}
              onClick={() => onSelect(item.id)}
              onDoubleClick={() => props.onQuickLook(item.id)}
            >
              <span className="row__name">
                <span className="row__name-text" title={item.name}>
                  {item.is_dir ? "📁 " : ""}
                  {item.name}
                </span>
                {item.duplicate_group !== null && <span className="chip chip--duplicate">Copy</span>}
                {item.sensitive && <span className="chip chip--attention">Credential</span>}
                {deletion && <span className={`chip chip--${deletion.confidence}`}>{deletion.confidence}</span>}
              </span>
              <span className="row__size num">
                {formatBytes(item.size_bytes)}
                <span
                  className="size-bar"
                  style={{ transform: `scaleX(${Math.max(0.02, item.size_bytes / largest)})` }}
                  aria-hidden="true"
                />
              </span>
              <span className="row__age">{formatAge(item.modified_at, now)}</span>
              <DecisionControl
                decision={decision}
                plannedFolder={planned.get(item.id) ?? null}
                onChange={(d) => props.onDecide(item.id, d)}
              />
            </div>
          );
        })}
      </div>
    </section>
  );
}
