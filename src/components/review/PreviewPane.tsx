import { useEffect, useRef, useState } from "react";
import { api, errorMessage, type FileItem, type Plan } from "../../lib/api";
import { formatAge, formatBytes, nowSeconds } from "../../lib/format";
import type { Decision, DecisionMap } from "../../lib/review";
import { DecisionControl } from "./DecisionControl";

interface PreviewPaneProps {
  item: FileItem | null;
  items: readonly FileItem[];
  plan: Plan;
  decisions: DecisionMap;
  onDecide: (id: number, decision: Decision) => void;
  onError: (message: string) => void;
}

type Thumb = { status: "loading" } | { status: "ready"; url: string | null };

function useThumbnail(itemId: number | null): Thumb {
  const cache = useRef(new Map<number, string | null>());
  const [thumb, setThumb] = useState<Thumb>({ status: "loading" });

  useEffect(() => {
    if (itemId === null) return;
    const cached = cache.current.get(itemId);
    if (cached !== undefined) {
      setThumb({ status: "ready", url: cached });
      return;
    }
    let isCurrent = true;
    setThumb({ status: "loading" });
    api
      .thumbnail(itemId)
      .catch(() => null)
      .then((url) => {
        cache.current.set(itemId, url);
        if (isCurrent) setThumb({ status: "ready", url });
      });
    return () => {
      isCurrent = false;
    };
  }, [itemId]);

  return thumb;
}

export function PreviewPane({ item, items, plan, decisions, onDecide, onError }: PreviewPaneProps) {
  const thumb = useThumbnail(item?.id ?? null);

  if (!item) {
    return (
      <aside className="preview" aria-label="Preview">
        <p className="empty-list">Select a file to preview it.</p>
      </aside>
    );
  }

  const now = nowSeconds();
  const decision = decisions[item.id] ?? { kind: "keep" };
  const move = plan.moves.find((m) => m.item_id === item.id);
  const deletion = plan.deletions.find((d) => d.item_id === item.id);
  const copies =
    item.duplicate_group === null
      ? []
      : items.filter((i) => i.duplicate_group === item.duplicate_group && i.id !== item.id);
  const run = (action: () => Promise<void>) => action().catch((e) => onError(errorMessage(e)));

  return (
    <aside className="preview" aria-label="Preview">
      <div className="preview__stage">
        {thumb.status === "ready" && thumb.url ? (
          <img src={thumb.url} alt={`Preview of ${item.name}`} />
        ) : (
          <span className="preview__kind" aria-hidden="true">
            {thumb.status === "loading" ? "…" : item.is_dir ? "DIR" : item.extension.slice(0, 4) || "FILE"}
          </span>
        )}
      </div>

      <div>
        <h3 className="preview__name">{item.name}</h3>
        <dl className="preview__facts">
          <dt>Size</dt>
          <dd className="num">
            {formatBytes(item.size_bytes)}
            {item.is_dir && ` · ${item.file_count.toLocaleString()} files`}
          </dd>
          <dt>Modified</dt>
          <dd>{formatAge(item.modified_at, now)}</dd>
          <dt>Opened</dt>
          <dd>{formatAge(item.accessed_at, now)}</dd>
        </dl>
      </div>

      <div style={{ display: "grid", gap: "var(--space-3)", alignContent: "start" }}>
        <div className="preview__actions">
          <DecisionControl
            decision={decision}
            plannedFolder={move?.folder ?? null}
            onChange={(d) => onDecide(item.id, d)}
          />
          <select
            className="field"
            aria-label="Move to folder"
            value={decision.kind === "move" ? decision.folder : ""}
            onChange={(e) => e.target.value && onDecide(item.id, { kind: "move", folder: e.target.value })}
          >
            <option value="" disabled>
              Move to…
            </option>
            {plan.folders.map((f) => (
              <option key={f.name} value={f.name}>
                {f.name}
              </option>
            ))}
          </select>
        </div>

        {item.sensitive && (
          <p className="reason reason--attention">
            <strong>Needs attention</strong>
            This looks like a credential or key. Moving it doesn't make it safer: store it in a password manager or
            secrets vault, or delete it and rotate the key if it's no longer used.
          </p>
        )}
        {deletion && (
          <p className="reason reason--trash">
            <strong>Suggested for Trash · {deletion.confidence} confidence</strong>
            {deletion.reason}
          </p>
        )}
        {move && (
          <p className="reason">
            <strong>Why {move.folder}</strong>
            {move.reason}
          </p>
        )}
        {copies.length > 0 && (
          <div className="reason reason--trash" style={{ borderLeftColor: "var(--color-violet)" }}>
            <strong>Byte-identical copies (verified)</strong>
            <ul className="preview__copies">
              {copies.map((c) => (
                <li key={c.id}>
                  {c.name} — {decisions[c.id]?.kind === "trash" ? "marked for Trash" : "kept"}
                </li>
              ))}
            </ul>
          </div>
        )}

        <div className="preview__actions">
          <button className="btn" onClick={() => run(() => api.quickLook(item.id))}>
            Quick Look
          </button>
          <button className="btn" onClick={() => run(() => api.revealInFinder(item.id))}>
            Show in Finder
          </button>
        </div>
      </div>
    </aside>
  );
}
