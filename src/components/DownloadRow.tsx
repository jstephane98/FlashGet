import type { Download, Priority } from "../types";
import { formatSpeed, percent } from "../format";
import { PriorityPill } from "./PriorityPill";
import { CategoryPill } from "./CategoryPill";
import { RowMenu } from "./RowMenu";

const STATUS_LABEL: Record<string, string> = {
  queued: "En file",
  downloading: "En cours",
  paused: "En pause",
  completed: "Terminé",
  error: "Erreur",
};

export function DownloadRow({
  d,
  speed,
  queuePosition,
  seg,
  onPause,
  onResume,
  onCancel,
  onPriority,
  onOpenFile,
  onOpenFolder,
  onRemove,
}: {
  d: Download;
  speed: number;
  queuePosition: number | null;
  seg?: { done: number; total: number };
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onPriority: (p: Priority) => void;
  onOpenFile: () => void;
  onOpenFolder: () => void;
  onRemove: () => void;
}) {
  const isSegmented = d.kind === "hls" || d.kind === "dash";
  const segPct = seg && seg.total > 0 ? Math.round((seg.done / seg.total) * 100) : 0;
  const pct = isSegmented && seg ? segPct : percent(d.downloaded_bytes, d.total_bytes);
  const stateLabel =
    d.status === "queued" && queuePosition !== null
      ? `En file (${queuePosition})`
      : isSegmented && seg && d.status === "downloading"
        ? `En cours · ${seg.done}/${seg.total} segments`
        : `${STATUS_LABEL[d.status]} · ${pct}%`;
  return (
    <div className="flex items-center gap-3 px-3 py-2 border-b border-gray-100 text-sm">
      <div className="flex-[2] min-w-0">
        <div className="font-semibold truncate">
          {d.kind === "hls" && (
            <span className="mr-1 px-1 py-0.5 text-[10px] rounded bg-indigo-100 text-indigo-700 align-middle">HLS</span>
          )}
          {d.kind === "dash" && (
            <span className="mr-1 px-1 py-0.5 text-[10px] rounded bg-violet-100 text-violet-700 align-middle">DASH</span>
          )}
          {d.file_name}
        </div>
        <div className="h-1.5 mt-1 rounded bg-gray-200 overflow-hidden">
          <div className={`h-full rounded ${d.status === "completed" ? "bg-green-500" : "bg-indigo-600"}`} style={{ width: `${pct}%` }} />
        </div>
      </div>
      <div className="flex-1"><CategoryPill value={d.category} /></div>
      <div className="flex-1"><PriorityPill value={d.priority} onChange={onPriority} /></div>
      <div className="flex-1">{stateLabel}</div>
      <div className="flex-1">{d.status === "downloading" ? formatSpeed(speed) : "—"}</div>
      <div className="flex gap-1 items-center">
        {d.status === "downloading" ? (
          <button className="px-2 py-1 rounded bg-gray-100" onClick={onPause}>⏸</button>
        ) : d.status !== "completed" ? (
          <button className="px-2 py-1 rounded bg-gray-100" onClick={onResume}>▶</button>
        ) : null}
        <button className="px-2 py-1 rounded bg-gray-100" onClick={onCancel} title="Annuler (efface le fichier)">✕</button>
        <RowMenu canOpen={d.status === "completed"} onOpenFile={onOpenFile} onOpenFolder={onOpenFolder} onRemove={onRemove} />
      </div>
    </div>
  );
}
