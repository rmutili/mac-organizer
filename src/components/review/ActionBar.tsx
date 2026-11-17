import { formatBytes } from "../../lib/format";
import type { Summary } from "../../lib/review";

interface ActionBarProps {
  summary: Summary;
  isBusy: boolean;
  onApply: () => void;
}

export function ActionBar({ summary, isBusy, onApply }: ActionBarProps) {
  const isEmpty = summary.moves === 0 && summary.trash === 0;
  return (
    <footer className="action-bar">
      <span className="action-bar__summary num">
        <strong>{summary.moves.toLocaleString()}</strong> to move ·{" "}
        <strong>{summary.trash.toLocaleString()}</strong> to Trash
        {summary.freedBytes > 0 && (
          <>
            {" "}
            · frees <span className="action-bar__freed">{formatBytes(summary.freedBytes)}</span>
          </>
        )}
      </span>
      <span className="kbd-hint">
        <kbd>↑</kbd> <kbd>↓</kbd> browse · <kbd>Space</kbd> Quick Look · <kbd>⌫</kbd> Trash · <kbd>K</kbd> keep
      </span>
      <button className="btn btn--primary" onClick={onApply} disabled={isBusy || isEmpty}>
        Review & Apply…
      </button>
    </footer>
  );
}
