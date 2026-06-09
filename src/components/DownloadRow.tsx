import { useEffect, useRef, useState } from "react";
import type { Download, Priority } from "../types";
import { formatSpeed, percent, recordingLabel } from "../format";
import { PriorityPill } from "./PriorityPill";
import { CategoryPill } from "./CategoryPill";
import { RowMenu } from "./RowMenu";

const STATUS_LABEL: Record<string, string> = {
  queued: "En file",
  downloading: "En cours",
  recording: "Enregistrement",
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
  onStop,
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
  onStop: () => void;
  onPriority: (p: Priority) => void;
  onOpenFile: () => void;
  onOpenFolder: () => void;
  onRemove: () => void;
}) {
  const isRecording = d.status === "recording";

  // Elapsed timer: stamp a start time the first time this row is seen recording,
  // tick every second, and clean up when it leaves recording / unmounts.
  const startRef = useRef<number | null>(null);
  const [, force] = useState(0);
  useEffect(() => {
    if (!isRecording) {
      startRef.current = null;
      return;
    }
    if (startRef.current === null) startRef.current = Date.now();
    const t = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [isRecording]);
  const elapsed = startRef.current ? Math.floor((Date.now() - startRef.current) / 1000) : 0;

  const isSegmented = d.kind === "hls" || d.kind === "dash";
  const segPct = seg && seg.total > 0 ? Math.round((seg.done / seg.total) * 100) : 0;
  // Un téléchargement terminé affiche toujours une barre pleine, même si la taille
  // totale était inconnue (cas yt-dlp : total_bytes = NA sur les flux séparés).
  const measurable = (isSegmented && seg && seg.total > 0) || (d.total_bytes != null && d.total_bytes > 0);
  const pct = d.status === "completed" ? 100 : isSegmented && seg ? segPct : percent(d.downloaded_bytes, d.total_bytes);
  // Pendant un téléchargement sans taille connue, barre indéterminée (animée) plutôt que blanche.
  const indeterminate = !isRecording && d.status === "downloading" && !measurable;
  const stateLabel = isRecording
    ? recordingLabel(seg?.done ?? 0, d.downloaded_bytes, elapsed)
    : d.status === "queued" && queuePosition !== null
      ? `En file (${queuePosition})`
      : isSegmented && seg && d.status === "downloading"
        ? `En cours · ${seg.done}/${seg.total} segments`
        : `${STATUS_LABEL[d.status]} · ${pct}%`;
  // Status indicator dot classes
  const dotClass =
    isRecording
      ? "bg-err animate-pulse"
      : d.status === "downloading"
        ? "bg-accent shadow-[0_0_8px_#f59e0b]"
        : d.status === "completed"
          ? "bg-ok"
          : d.status === "paused"
            ? "bg-accent-soft"
            : d.status === "queued"
              ? "bg-queued"
              : d.status === "error"
                ? "bg-err"
                : "bg-ink-mute";

  // Progress bar fill classes
  const barFill =
    isRecording
      ? "bg-err animate-pulse"
      : indeterminate
        ? "bg-accent animate-pulse"
        : d.status === "completed"
          ? "bg-okdim transition-[width] duration-300"
          : d.status === "paused"
            ? "bg-accent-soft transition-[width] duration-300"
            : d.status === "queued"
              ? "bg-queued transition-[width] duration-300"
              : d.status === "error"
                ? "bg-err transition-[width] duration-300"
                : "bg-accent transition-[width] duration-300";

  return (
    <div className="flex items-center gap-3 px-4 py-3 border-b border-border text-sm hover:bg-surface/60">
      <div className="flex-[2] min-w-0">
        <div className="flex items-center gap-1.5 font-semibold truncate text-ink">
          {/* Status dot */}
          <span className={`inline-block shrink-0 w-2 h-2 rounded-full ${dotClass}`} />
          {d.kind === "hls" && (
            <span className="px-1 py-0.5 text-[10px] rounded bg-surface text-ink-soft border border-border align-middle">HLS</span>
          )}
          {d.kind === "dash" && (
            <span className="px-1 py-0.5 text-[10px] rounded bg-surface text-ink-soft border border-border align-middle">DASH</span>
          )}
          {d.kind === "ytdlp" && (
            <span className="px-1 py-0.5 text-[10px] rounded bg-surface text-accent-bright border border-border align-middle">yt-dlp</span>
          )}
          {isRecording && (
            <span className="px-1 py-0.5 text-[10px] rounded bg-err/20 text-err border border-err/40 align-middle">LIVE</span>
          )}
          {d.file_name}
        </div>
        <div className="h-1.5 mt-1.5 rounded bg-surface overflow-hidden">
          {isRecording ? (
            <div className={`h-full w-1/3 rounded ${barFill}`} />
          ) : indeterminate ? (
            <div className={`h-full w-1/3 rounded ${barFill}`} />
          ) : (
            <div className={`h-full rounded ${barFill}`} style={{ width: `${pct}%` }} />
          )}
        </div>
      </div>
      <div className="flex-1"><CategoryPill value={d.category} /></div>
      <div className="flex-1"><PriorityPill value={d.priority} onChange={onPriority} /></div>
      <div className="flex-1 font-mono text-ink-soft text-xs">{stateLabel}</div>
      <div className="flex-1 font-mono text-xs">
        {d.status === "downloading" || isRecording
          ? <span className="text-accent-bright">{formatSpeed(speed)}</span>
          : <span className="text-ink-mute">—</span>}
      </div>
      <div className="flex gap-1 items-center">
        {isRecording ? (
          <button className="px-2 py-1 rounded bg-surface text-ink-soft hover:text-ink" onClick={onStop} title="Arrêter (finalise l'enregistrement)">⏹</button>
        ) : d.status === "downloading" ? (
          <button className="px-2 py-1 rounded bg-surface text-ink-soft hover:text-ink" onClick={onPause}>⏸</button>
        ) : d.status !== "completed" ? (
          <button className="px-2 py-1 rounded bg-surface text-ink-soft hover:text-ink" onClick={onResume}>▶</button>
        ) : null}
        <button className="px-2 py-1 rounded bg-surface text-ink-soft hover:text-ink" onClick={onCancel} title="Annuler (efface le fichier)">✕</button>
        <RowMenu canOpen={d.status === "completed"} onOpenFile={onOpenFile} onOpenFolder={onOpenFolder} onRemove={onRemove} />
      </div>
    </div>
  );
}
