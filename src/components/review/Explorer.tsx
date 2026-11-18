import { useMemo, useState } from "react";
import { api, errorMessage, type Plan, type ScanResponse } from "../../lib/api";
import { formatBytes } from "../../lib/format";
import { type Decision, type DecisionMap, type View, itemsInView, viewCounts, viewKey } from "../../lib/review";
import { HeatmapView } from "../overview/HeatmapView";
import { FileList, type SortKey, sortItems } from "./FileList";
import { PreviewPane } from "./PreviewPane";
import { Sidebar } from "./Sidebar";

interface ExplorerProps {
  scan: ScanResponse;
  plan: Plan;
  decisions: DecisionMap;
  onDecide: (id: number, decision: Decision) => void;
  onToggleTrash: (id: number) => void;
  onError: (message: string) => void;
}

const VIEW_COPY: Record<string, { title: string; subtitle: string }> = {
  "to-delete": {
    title: "Suggested for Trash",
    subtitle: "Claude's suggestions. Only high-confidence ones and verified copies are marked to start with.",
  },
  duplicates: {
    title: "Exact duplicates",
    subtitle: "Byte-identical files, verified by content hash. At least one copy of each is always kept by default.",
  },
  attention: {
    title: "Needs attention",
    subtitle: "These look like credentials or keys. They're never suggested for the Trash automatically.",
  },
  unchanged: {
    title: "Stays where it is",
    subtitle: "Items that won't be moved or trashed.",
  },
};

export function Explorer({ scan, plan, decisions, onDecide, onToggleTrash, onError }: ExplorerProps) {
  const [view, setView] = useState<View>({ kind: "to-delete" });
  const [sort, setSort] = useState<SortKey>("size");
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const counts = useMemo(() => viewCounts(scan.items, plan, decisions), [scan, plan, decisions]);
  const visible = useMemo(
    () => sortItems(itemsInView(view, scan.items, plan, decisions), sort),
    [view, scan, plan, decisions, sort],
  );
  const selected = scan.items.find((i) => i.id === selectedId) ?? visible[0] ?? null;

  const count = counts[viewKey(view)] ?? { count: 0, bytes: 0 };
  const folder = view.kind === "folder" ? plan.folders.find((f) => f.name === view.name) : undefined;
  const copy = folder
    ? { title: folder.name, subtitle: folder.description }
    : view.kind === "unchanged"
      ? { ...VIEW_COPY.unchanged, title: `Stays in ${scan.name}` }
      : (VIEW_COPY[view.kind] ?? { title: "", subtitle: "" });
  const subtitle = `${count.count.toLocaleString()} items · ${formatBytes(count.bytes)}${copy.subtitle ? ` · ${copy.subtitle}` : ""}`;

  const quickLook = (id: number) => api.quickLook(id).catch((e) => onError(errorMessage(e)));

  return (
    <div className="explorer">
      <Sidebar
        plan={plan}
        folderName={scan.name}
        counts={counts}
        view={view}
        onSelect={(v) => {
          setView(v);
          setSelectedId(null);
        }}
      />
      {view.kind === "overview" ? (
        <div style={{ gridColumn: "2 / -1", display: "grid", minHeight: 0 }}>
          <HeatmapView scan={scan} />
        </div>
      ) : (
        <>
          <FileList
            title={copy.title}
            subtitle={subtitle}
            items={visible}
            plan={plan}
            decisions={decisions}
            selectedId={selected?.id ?? null}
            sort={sort}
            onSort={setSort}
            onSelect={setSelectedId}
            onDecide={onDecide}
            onToggleTrash={onToggleTrash}
            onQuickLook={quickLook}
          />
          <PreviewPane
            item={selected}
            items={scan.items}
            plan={plan}
            decisions={decisions}
            onDecide={onDecide}
            onError={onError}
          />
        </>
      )}
    </div>
  );
}
