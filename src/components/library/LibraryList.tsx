import type { Download } from "../../types";
import { formatLabel, type DayGroup } from "../../library/library";
import { formatBytes, hostOf } from "../../format";
import { Thumb } from "./Thumb";

const BADGE = "shrink-0 px-1.5 py-0.5 text-[10px] rounded bg-surface text-ink-soft border border-border";

function timeOf(d: Download): string {
  if (d.completed_at == null) return "—";
  const date = new Date(d.completed_at * 1000);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

export function LibraryList({
  groups,
  selectedId,
  onSelect,
}: {
  groups: DayGroup[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="flex flex-col gap-4">
      {groups.map((g) => (
        <section key={g.key}>
          <h3 className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-ink-mute">{g.label}</h3>
          <div className="flex flex-col gap-1">
            {g.items.map((d) => {
              const selected = d.id === selectedId;
              const label = formatLabel(d);
              return (
                <button
                  key={d.id}
                  type="button"
                  aria-current={selected ? "true" : undefined}
                  onClick={() => onSelect(d.id)}
                  className={`w-full grid grid-cols-[64px_minmax(0,1fr)_auto_auto] items-center gap-3 px-3 py-2 rounded-lg border text-left text-sm ${
                    selected ? "bg-accent/10 border-accent" : "bg-panel border-line hover:border-border"
                  }`}
                >
                  <Thumb d={d} className="w-16 h-[38px]" />
                  <div className="min-w-0">
                    <div className="font-semibold text-ink truncate">{d.file_name}</div>
                    <div className="mt-0.5 flex items-center gap-2 text-xs text-ink-mute min-w-0">
                      {label && <span className={BADGE}>{label}</span>}
                      <span className="truncate">{hostOf(d.url)}</span>
                    </div>
                  </div>
                  <span className="font-mono text-xs text-ink-soft">{formatBytes(d.total_bytes ?? d.downloaded_bytes)}</span>
                  <span className="font-mono text-xs text-ink-mute w-10 text-right">{timeOf(d)}</span>
                </button>
              );
            })}
          </div>
        </section>
      ))}
    </div>
  );
}
