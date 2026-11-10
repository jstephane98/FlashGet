export type BarTone = "active" | "paused" | "error" | "done" | "queued" | "live";

const FILL: Record<BarTone, string> = {
  active: "bg-accent",
  paused: "bg-accent-soft",
  error: "bg-err",
  done: "bg-ok",
  queued: "bg-queued",
  live: "bg-err animate-pulse",
};

export function SegmentBar({
  parts,
  percent,
  tone,
  indeterminate,
}: {
  parts?: number[] | null;
  percent: number;
  tone: BarTone;
  indeterminate?: boolean;
}) {
  const fill = FILL[tone];
  if (indeterminate) {
    return (
      <div className="h-1.5 rounded bg-surface overflow-hidden">
        <div data-testid="bar-indeterminate" className={`h-full w-1/3 rounded animate-pulse ${fill}`} />
      </div>
    );
  }
  if (parts && parts.length >= 2) {
    return (
      <div className="flex gap-0.5 h-1.5">
        {parts.map((p, i) => (
          <div key={i} data-testid="segment" className="flex-1 rounded-sm bg-surface overflow-hidden">
            <div
              className={`h-full transition-[width] duration-300 ${fill}`}
              style={{ width: `${Math.max(0, Math.min(1, p)) * 100}%` }}
            />
          </div>
        ))}
      </div>
    );
  }
  return (
    <div className="h-1.5 rounded bg-surface overflow-hidden">
      <div
        data-testid="bar-fill"
        className={`h-full rounded transition-[width] duration-300 ${fill}`}
        style={{ width: `${percent}%` }}
      />
    </div>
  );
}
