import { useEffect } from "react";
import type { ApplyReport, UndoReport } from "../../lib/api";
import { formatBytes } from "../../lib/format";
import type { Summary } from "../../lib/review";

type SheetProps =
  | {
      mode: "confirm";
      folderName: string;
      summary: Summary;
      allCopiesTrashed: number;
      isBusy: boolean;
      onConfirm: () => void;
      onCancel: () => void;
    }
  | { mode: "applied"; report: ApplyReport; onUndo: () => void; onDone: () => void }
  | { mode: "undone"; report: UndoReport; onDone: () => void };

function Stat({ value, label }: { value: string; label: string }) {
  return (
    <div className="stat">
      <span className="stat__value num">{value}</span>
      <span className="stat__label">{label}</span>
    </div>
  );
}

function Failures({ failures }: { failures: { name: string; error: string }[] }) {
  if (failures.length === 0) return null;
  return (
    <>
      <p>
        {failures.length} item{failures.length === 1 ? "" : "s"} couldn't be changed:
      </p>
      <ul className="sheet__failures">
        {failures.map((f) => (
          <li key={`${f.name}-${f.error}`}>
            <strong>{f.name}</strong>: {f.error}
          </li>
        ))}
      </ul>
    </>
  );
}

export function ApplySheet(props: SheetProps) {
  const dismiss = props.mode === "confirm" ? props.onCancel : props.onDone;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && dismiss();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dismiss]);

  return (
    <div className="sheet-backdrop" role="presentation">
      <div className="sheet" role="dialog" aria-modal="true" aria-labelledby="sheet-title">
        {props.mode === "confirm" && (
          <>
            <h2 id="sheet-title">Apply these changes?</h2>
            <p>
              Files are moved within {props.folderName}, and anything marked goes to the Trash. You can undo this
              afterwards.
            </p>
            <div className="sheet__stats">
              <Stat value={props.summary.moves.toLocaleString()} label="moved into folders" />
              <Stat value={props.summary.trash.toLocaleString()} label="to the Trash" />
              <Stat value={formatBytes(props.summary.freedBytes)} label="freed" />
            </div>
            {props.allCopiesTrashed > 0 && (
              <p style={{ color: "var(--color-danger)" }}>
                Every copy of {props.allCopiesTrashed} duplicated file{props.allCopiesTrashed === 1 ? " is" : "s are"}{" "}
                marked for the Trash, so no copy will stay in {props.folderName}.
              </p>
            )}
            <div className="sheet__actions">
              <button className="btn" onClick={props.onCancel} disabled={props.isBusy}>
                Cancel
              </button>
              <button className="btn btn--primary" onClick={props.onConfirm} disabled={props.isBusy} autoFocus>
                {props.isBusy ? "Applying…" : "Apply"}
              </button>
            </div>
          </>
        )}

        {props.mode === "applied" && (
          <>
            <h2 id="sheet-title">Done</h2>
            <div className="sheet__stats">
              <Stat value={props.report.moved.toLocaleString()} label="moved" />
              <Stat value={props.report.trashed.toLocaleString()} label="in the Trash" />
              <Stat value={formatBytes(props.report.freed_bytes)} label="freed" />
            </div>
            <Failures failures={props.report.failures} />
            <div className="sheet__actions">
              <button className="btn btn--danger" onClick={props.onUndo}>
                Undo
              </button>
              <button className="btn btn--primary" onClick={props.onDone} autoFocus>
                Scan again
              </button>
            </div>
          </>
        )}

        {props.mode === "undone" && (
          <>
            <h2 id="sheet-title">Undone</h2>
            <p>{props.report.restored.toLocaleString()} items are back where they were.</p>
            <Failures failures={props.report.failures} />
            <div className="sheet__actions">
              <button className="btn btn--primary" onClick={props.onDone} autoFocus>
                Scan again
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
