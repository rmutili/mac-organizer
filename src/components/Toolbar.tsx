import { MODELS, type ClassifyProgress, type ModelId, type ScanProgress } from "../lib/api";

export type Busy = "scanning" | "organizing" | "applying" | null;

interface ToolbarProps {
  busy: Busy;
  hasScan: boolean;
  canOrganize: boolean;
  backendLabel: string;
  model: ModelId;
  scanProgress: ScanProgress | null;
  classifyProgress: ClassifyProgress | null;
  onModel: (model: ModelId) => void;
  onScan: () => void;
  onOrganize: () => void;
}

function Progress({ fraction, label }: { fraction: number; label: string }) {
  return (
    <div className="progress" role="status" aria-live="polite">
      <div className="progress__bar">
        <div className="progress__fill" style={{ transform: `scaleX(${Math.min(1, Math.max(0, fraction))})` }} />
      </div>
      <span className="progress__label" title={label}>
        {label}
      </span>
    </div>
  );
}

function scanLabel(p: ScanProgress): string {
  const position = `${(p.done + 1).toLocaleString()} of ${p.total.toLocaleString()}`;
  return p.phase === "listing"
    ? `Scanning ${p.name} · ${position}`
    : `Checking for exact duplicates · ${p.name} · ${position}`;
}

function classifyLabel(p: ClassifyProgress): string {
  if (p.done === 0 && p.sorting.length === 0) return "Claude is designing folders from all file names…";
  const more = p.sorting_count - p.sorting.length;
  const sorting = p.sorting.length > 0 ? ` · Sorting ${p.sorting.join(", ")}${more > 0 ? ` … +${more} more` : ""}` : "";
  return `${p.done} of ${p.total} steps done${sorting}`;
}

export function Toolbar(props: ToolbarProps) {
  const { busy, scanProgress: scan, classifyProgress: classify } = props;
  return (
    <header className="toolbar">
      <h1 className="toolbar__title">Mac Organizer</h1>

      {busy === "scanning" && scan && scan.total > 0 && (
        <Progress fraction={(scan.done + 1) / scan.total} label={scanLabel(scan)} />
      )}
      {busy === "organizing" && classify && (
        <Progress fraction={classify.done / Math.max(1, classify.total)} label={classifyLabel(classify)} />
      )}
      {busy === "applying" && <Progress fraction={1} label="Applying changes…" />}
      {!busy && <span className="toolbar__meta">{props.backendLabel}</span>}

      <select
        className="field"
        aria-label="Claude model"
        value={props.model}
        disabled={busy !== null}
        onChange={(e) => props.onModel(e.target.value as ModelId)}
      >
        {MODELS.map((m) => (
          <option key={m.id} value={m.id}>
            {m.label}
          </option>
        ))}
      </select>
      <button className="btn" onClick={props.onScan} disabled={busy !== null}>
        {props.hasScan ? "Rescan" : "Scan Downloads"}
      </button>
      <button className="btn btn--primary" onClick={props.onOrganize} disabled={busy !== null || !props.canOrganize}>
        Organize with Claude
      </button>
    </header>
  );
}
