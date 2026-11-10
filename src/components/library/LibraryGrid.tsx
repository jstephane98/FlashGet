import type { Download } from "../../types";
import { formatLabel } from "../../library/library";
import { formatBytes } from "../../format";
import { Thumb } from "./Thumb";

const BADGE = "shrink-0 px-1.5 py-0.5 text-[10px] rounded bg-surface text-ink-soft border border-border";

export function LibraryGrid({
  items,
  selectedId,
  onSelect,
}: {
  items: Download[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(190px, 1fr))" }}>
      {items.map((d) => {
        const selected = d.id === selectedId;
        const label = formatLabel(d);
        const date = d.completed_at == null ? "—"
          : new Date(d.completed_at * 1000).toLocaleDateString("fr-FR", { day: "numeric", month: "short" });
        return (
          <button
            key={d.id}
            type="button"
            aria-current={selected ? "true" : undefined}
            onClick={() => onSelect(d.id)}
            className={`flex flex-col gap-2 p-2 rounded-lg border text-left text-sm ${
              selected ? "bg-accent/10 border-accent" : "bg-panel border-line hover:border-border"
            }`}
          >
            <Thumb d={d} className="w-full aspect-video" iconSize={28} />
            <div className="font-semibold text-ink truncate">{d.file_name}</div>
            <div className="flex items-center gap-2 text-xs text-ink-mute">
              {label && <span className={BADGE}>{label}</span>}
              <span className="font-mono">{formatBytes(d.total_bytes ?? d.downloaded_bytes)}</span>
              <span className="ml-auto">{date}</span>
            </div>
          </button>
        );
      })}
    </div>
  );
}
