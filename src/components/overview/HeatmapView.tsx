import { useMemo, useState } from "react";
import type { AgeBucket, Category, ScanResponse } from "../../lib/api";
import { formatBytes } from "../../lib/format";

const AGES: { id: AgeBucket; label: string }[] = [
  { id: "week", label: "This week" },
  { id: "month", label: "This month" },
  { id: "quarter", label: "1–3 months" },
  { id: "year", label: "3–12 months" },
  { id: "older", label: "Over a year" },
];

/** Cells darker than this get white text. */
const STRONG_THRESHOLD = 0.6;

type Measure = "size" | "count";

interface HeatmapViewProps {
  scan: ScanResponse;
  footer?: React.ReactNode;
}

export function HeatmapView({ scan, footer }: HeatmapViewProps) {
  const [measure, setMeasure] = useState<Measure>("size");
  const value = (cell: { size_bytes: number; file_count: number }) =>
    measure === "size" ? cell.size_bytes : cell.file_count;

  const { rows, max } = useMemo(() => {
    const byCategory = new Map<Category, Map<AgeBucket, { size_bytes: number; file_count: number }>>();
    for (const cell of scan.heatmap.cells) {
      const row = byCategory.get(cell.category) ?? new Map();
      byCategory.set(cell.category, new Map(row).set(cell.age, cell));
    }
    const rows = [...byCategory.entries()]
      .map(([category, cells]) => ({
        category,
        cells,
        total: [...cells.values()].reduce((sum, c) => sum + value(c), 0),
      }))
      .filter((row) => row.total > 0)
      .sort((a, b) => b.total - a.total);
    const max = Math.max(1, ...rows.flatMap((r) => [...r.cells.values()].map(value)));
    return { rows, max };
  }, [scan, measure]);

  const format = (n: number) => (measure === "size" ? formatBytes(n) : n.toLocaleString());

  return (
    <section className="overview" aria-labelledby="overview-title">
      <header className="overview__header">
        <div>
          <h2 id="overview-title" className="overview__total num">
            {measure === "size"
              ? formatBytes(scan.heatmap.total_size_bytes)
              : `${scan.heatmap.total_file_count.toLocaleString()} files`}
          </h2>
          <p className="overview__caption">
            {scan.items.length.toLocaleString()} items in Downloads, by type and by when they were last modified
          </p>
        </div>
        <div className="segmented" role="group" aria-label="Measure">
          <button aria-pressed={measure === "size"} onClick={() => setMeasure("size")}>
            Space
          </button>
          <button aria-pressed={measure === "count"} onClick={() => setMeasure("count")}>
            Files
          </button>
        </div>
      </header>

      <div className="heatmap" role="table" aria-label="Downloads by type and age">
        <div role="row" style={{ display: "contents" }}>
          <span />
          {AGES.map((age) => (
            <span key={age.id} className="heatmap__col-label" role="columnheader">
              {age.label}
            </span>
          ))}
          <span className="heatmap__col-label" role="columnheader">
            Total
          </span>
        </div>
        {rows.map((row) => (
          <div key={row.category} role="row" style={{ display: "contents" }}>
            <span className="heatmap__row-label" role="rowheader">
              {row.category}
            </span>
            {AGES.map((age) => {
              const cell = row.cells.get(age.id);
              const amount = cell ? value(cell) : 0;
              // Square-root scale so small-but-real cells stay visible next to huge ones.
              const heat = Math.sqrt(amount / max);
              const classes = [
                "heatmap__cell",
                amount === 0 && "heatmap__cell--empty",
                heat > STRONG_THRESHOLD && "heatmap__cell--strong",
              ]
                .filter(Boolean)
                .join(" ");
              return (
                <span
                  key={age.id}
                  role="cell"
                  className={classes}
                  style={{ "--heat": heat.toFixed(3) } as React.CSSProperties}
                  title={`${row.category}, ${age.label.toLowerCase()}: ${cell ? formatBytes(cell.size_bytes) : "0 B"}, ${cell?.file_count ?? 0} files`}
                >
                  {amount > 0 ? format(amount) : "·"}
                </span>
              );
            })}
            <span className="heatmap__row-total" role="cell">
              {format(row.total)}
            </span>
          </div>
        ))}
      </div>

      <div className="overview__legend" aria-hidden="true">
        <span>Less</span>
        <span className="overview__legend-ramp" />
        <span>More</span>
      </div>

      {footer}
    </section>
  );
}
