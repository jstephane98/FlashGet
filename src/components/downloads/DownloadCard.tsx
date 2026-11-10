import { useEffect, useRef, useState } from "react";
import { Square, Pause, Play, FolderOpen } from "lucide-react";
import type { Download, Priority } from "../../types";
import { formatBytes, formatElapsed, formatEta, formatSpeed, hostOf, percent } from "../../format";
import { t } from "../../i18n";
import { CategoryIcon } from "../categoryIcons";
import { SegmentBar, type BarTone } from "./SegmentBar";
import { RowMenu } from "./RowMenu";

const BTN = "inline-flex items-center justify-center w-7 h-7 rounded bg-field text-ink-soft hover:text-ink";
const BADGE = "shrink-0 px-1.5 py-0.5 text-[10px] rounded bg-surface text-ink-soft border border-border";

export function DownloadCard({
  d,
  speed,
  parts,
  seg,
  queuePosition,
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
  parts?: number[] | null;
  seg?: { done: number; total: number };
  queuePosition: number | null;
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

  // Minuteur du direct : horodatage à la première apparition en enregistrement, tic chaque seconde.
  const startRef = useRef<number | null>(null);
  const [, force] = useState(0);
  useEffect(() => {
    if (!isRecording) {
      startRef.current = null;
      return;
    }
    if (startRef.current === null) startRef.current = Date.now();
    const timer = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(timer);
  }, [isRecording]);
  const elapsed = startRef.current ? Math.floor((Date.now() - startRef.current) / 1000) : 0;

  const isSegmented = d.kind === "hls" || d.kind === "dash";
  const segPct = seg && seg.total > 0 ? Math.round((seg.done / seg.total) * 100) : 0;
  // Terminé = barre pleine, même si la taille totale était inconnue (yt-dlp).
  const measurable = (isSegmented && seg && seg.total > 0) || (d.total_bytes != null && d.total_bytes > 0);
  const pct = d.status === "completed" ? 100 : isSegmented && seg ? segPct : percent(d.downloaded_bytes, d.total_bytes);
  // Un direct n'a pas de progression mesurable : barre pulsée visible.
  const indeterminate = isRecording || (d.status === "downloading" && !measurable);
  const sizeKnown = d.total_bytes != null && d.total_bytes > 0;

  const tone: BarTone = isRecording
    ? "live"
    : d.status === "completed" ? "done"
    : d.status === "paused" ? "paused"
    : d.status === "error" ? "error"
    : d.status === "queued" ? "queued"
    : "active";

  const showParts = (d.status === "downloading" || d.status === "paused") && parts && parts.length >= 2 ? parts : null;
  const badge = d.kind === "hls" ? "HLS" : d.kind === "dash" ? "DASH" : d.kind === "ytdlp" ? "yt-dlp" : t(`cat.${d.category}` as const);

  const stateMeta = isRecording
    ? t("dl.live")
    : d.status === "queued" && queuePosition !== null
      ? t("dl.queuePos", { n: queuePosition })
      : showParts
        ? t("dl.connections", { n: showParts.length })
        : d.status === "error" && d.error
          ? d.error
          : null;

  const left = isRecording
    ? `${formatElapsed(elapsed)} · ${formatBytes(d.downloaded_bytes)}`
    : isSegmented && seg && seg.total > 0
      ? `${seg.done} / ${seg.total}`
      : sizeKnown
        ? `${formatBytes(d.downloaded_bytes)} / ${formatBytes(d.total_bytes as number)}`
        : formatBytes(d.downloaded_bytes);

  let right: string;
  if (isRecording) right = formatSpeed(speed);
  else if (d.status === "downloading") {
    right = sizeKnown
      ? `${formatSpeed(speed)} · ${formatEta((d.total_bytes as number) - d.downloaded_bytes, speed)}`
      : formatSpeed(speed);
  } else if (d.status === "completed") right = t("dl.done");
  else if (d.status === "paused") right = t("dl.paused");
  else if (d.status === "error") right = t("dl.interrupted");
  else right = t("dl.waiting");
  const rightActive = d.status === "downloading" || isRecording;

  return (
    <div className="grid grid-cols-[76px_minmax(0,1fr)_210px_auto] items-center gap-4 px-4 py-3 rounded-lg bg-panel border border-line text-sm">
      <div className="relative w-[76px] h-[46px] rounded-md bg-surface flex items-center justify-center text-ink-mute">
        <CategoryIcon cat={d.category} size={22} />
        {isRecording && (
          <span className="absolute top-1 left-1 px-1 text-[9px] rounded bg-err/20 text-err border border-err/40">LIVE</span>
        )}
      </div>
      <div className="min-w-0">
        <div className="font-semibold text-ink truncate">{d.file_name}</div>
        <div className="mt-1 flex items-center gap-2 text-xs text-ink-mute min-w-0">
          <span className={BADGE}>{badge}</span>
          <span className="truncate">{hostOf(d.url)}</span>
          {stateMeta && <span className={`truncate ${d.status === "error" ? "text-err" : ""}`}>{stateMeta}</span>}
        </div>
      </div>
      <div>
        <SegmentBar parts={showParts} percent={pct} tone={tone} indeterminate={indeterminate} />
        <div className="mt-1 flex justify-between gap-2 font-mono text-xs">
          <span className="text-ink-soft">{left}</span>
          <span className={rightActive ? "text-accent-fg" : "text-ink-mute"}>{right}</span>
        </div>
      </div>
      <div className="flex items-center gap-1">
        {isRecording ? (
          <button className={BTN} onClick={onStop} title={t("dl.stop")} aria-label={t("dl.stop")}><Square size={14} /></button>
        ) : d.status === "downloading" ? (
          <button className={BTN} onClick={onPause} title={t("dl.pause")} aria-label={t("dl.pause")}><Pause size={14} /></button>
        ) : d.status !== "completed" ? (
          <button className={BTN} onClick={onResume} title={t("dl.resume")} aria-label={t("dl.resume")}><Play size={14} /></button>
        ) : null}
        <button className={BTN} onClick={onOpenFolder} title={t("dl.openFolder")} aria-label={t("dl.openFolder")}><FolderOpen size={14} /></button>
        <RowMenu
          canOpen={d.status === "completed"}
          priority={d.priority}
          onOpenFile={onOpenFile}
          onOpenFolder={onOpenFolder}
          onPriority={onPriority}
          onCancel={onCancel}
          onRemove={onRemove}
        />
      </div>
    </div>
  );
}
