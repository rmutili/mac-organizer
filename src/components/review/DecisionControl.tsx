import type { Decision } from "../../lib/review";

interface DecisionControlProps {
  decision: Decision;
  /** Folder Claude planned for this item, if any. */
  plannedFolder: string | null;
  onChange: (decision: Decision) => void;
}

/** Move / Keep / Trash segmented control used in rows and the preview. */
export function DecisionControl({ decision, plannedFolder, onChange }: DecisionControlProps) {
  const moveTarget = decision.kind === "move" ? decision.folder : plannedFolder;
  return (
    <span className="decision" role="group" aria-label="Decision">
      <button
        data-kind="move"
        aria-pressed={decision.kind === "move"}
        disabled={moveTarget === null}
        title={moveTarget ? `Move to ${moveTarget}` : "Claude didn't suggest a folder"}
        onClick={(e) => {
          e.stopPropagation();
          if (moveTarget) onChange({ kind: "move", folder: moveTarget });
        }}
      >
        Move
      </button>
      <button
        data-kind="keep"
        aria-pressed={decision.kind === "keep"}
        title="Leave it where it is"
        onClick={(e) => {
          e.stopPropagation();
          onChange({ kind: "keep" });
        }}
      >
        Keep
      </button>
      <button
        data-kind="trash"
        aria-pressed={decision.kind === "trash"}
        title="Move to the Trash when you apply"
        onClick={(e) => {
          e.stopPropagation();
          onChange({ kind: "trash" });
        }}
      >
        Trash
      </button>
    </span>
  );
}
