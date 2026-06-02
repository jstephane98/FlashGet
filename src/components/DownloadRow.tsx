import type { Download } from "../types";
import { formatBytes, formatSpeed, percent } from "../format";

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
  onPause,
  onResume,
  onCancel,
}: {
  d: Download;
  speed: number;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
}) {
  const pct = percent(d.downloaded_bytes, d.total_bytes);
  return (
    <div className="flex items-center gap-3 px-3 py-2 border-b border-gray-100 text-sm">
      <div className="flex-[2] min-w-0">
        <div className="font-semibold truncate">{d.file_name}</div>
        <div className="h-1.5 mt-1 rounded bg-gray-200 overflow-hidden">
          <div
            className={`h-full rounded ${d.status === "completed" ? "bg-green-500" : "bg-indigo-600"}`}
            style={{ width: `${pct}%` }}
          />
        </div>
      </div>
      <div className="flex-1">{d.total_bytes ? formatBytes(d.total_bytes) : "—"}</div>
      <div className="flex-1">{STATUS_LABEL[d.status]} · {pct}%</div>
      <div className="flex-1">{d.status === "downloading" ? formatSpeed(speed) : "—"}</div>
      <div className="flex gap-1">
        {d.status === "downloading" ? (
          <button className="px-2 py-1 rounded bg-gray-100" onClick={onPause}>⏸</button>
        ) : d.status !== "completed" ? (
          <button className="px-2 py-1 rounded bg-gray-100" onClick={onResume}>▶</button>
        ) : null}
        <button className="px-2 py-1 rounded bg-gray-100" onClick={onCancel}>✕</button>
      </div>
    </div>
  );
}
