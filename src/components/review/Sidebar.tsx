import type { Plan } from "../../lib/api";
import { type View, type ViewCount, viewKey } from "../../lib/review";

interface SidebarProps {
  plan: Plan;
  folderName: string;
  counts: Record<string, ViewCount>;
  view: View;
  onSelect: (view: View) => void;
}

interface NavItemProps {
  label: string;
  dot: string;
  count?: number;
  isCurrent: boolean;
  onClick: () => void;
}

function NavItem({ label, dot, count, isCurrent, onClick }: NavItemProps) {
  return (
    <button className="nav-item" aria-current={isCurrent} onClick={onClick}>
      <span className={`dot dot--${dot}`} aria-hidden="true" />
      <span className="nav-item__label">{label}</span>
      {count !== undefined && <span className="nav-item__count">{count.toLocaleString()}</span>}
    </button>
  );
}

const REVIEW_VIEWS: { view: View; label: string; dot: string }[] = [
  { view: { kind: "to-delete" }, label: "Suggested for Trash", dot: "trash" },
  { view: { kind: "duplicates" }, label: "Exact duplicates", dot: "duplicate" },
  { view: { kind: "attention" }, label: "Needs attention", dot: "attention" },
];

export function Sidebar({ plan, folderName, counts, view, onSelect }: SidebarProps) {
  const current = viewKey(view);
  const folders = [...plan.folders].sort(
    (a, b) => (counts[`folder:${b.name}`]?.count ?? 0) - (counts[`folder:${a.name}`]?.count ?? 0),
  );

  return (
    <nav className="sidebar" aria-label="Review">
      <NavItem
        label="Overview"
        dot="overview"
        isCurrent={current === "overview"}
        onClick={() => onSelect({ kind: "overview" })}
      />

      <h3 className="sidebar__heading">Review</h3>
      {REVIEW_VIEWS.map(({ view: v, label, dot }) => (
        <NavItem
          key={viewKey(v)}
          label={label}
          dot={dot}
          count={counts[viewKey(v)]?.count}
          isCurrent={current === viewKey(v)}
          onClick={() => onSelect(v)}
        />
      ))}

      <h3 className="sidebar__heading">New folders</h3>
      {folders.map((folder) => {
        const v: View = { kind: "folder", name: folder.name };
        return (
          <NavItem
            key={folder.name}
            label={folder.name}
            dot="folder"
            count={counts[viewKey(v)]?.count}
            isCurrent={current === viewKey(v)}
            onClick={() => onSelect(v)}
          />
        );
      })}
      <NavItem
        label={`Stays in ${folderName}`}
        dot="keep"
        count={counts.unchanged?.count}
        isCurrent={current === "unchanged"}
        onClick={() => onSelect({ kind: "unchanged" })}
      />
    </nav>
  );
}
