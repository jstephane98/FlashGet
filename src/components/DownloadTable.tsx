import type { Download, Priority } from "../types";
import { DownloadRow } from "./DownloadRow";

const RANK: Record<Priority, number> = { high: 0, normal: 1, low: 2 };

export function DownloadTable({
  downloads,
  allDownloads,
  speeds,
  segments,
  onPause,
  onResume,
  onCancel,
  onStop,
  onPriority,
  onOpenFile,
  onOpenFolder,
  onRemove,
}: {
  downloads: Download[];
  allDownloads: Download[];
  speeds: Record<string, number>;
  segments: Record<string, { done: number; total: number }>;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onStop: (id: string) => void;
  onPriority: (id: string, p: Priority) => void;
  onOpenFile: (id: string) => void;
  onOpenFolder: (id: string) => void;
  onRemove: (id: string) => void;
}) {
  const queued = allDownloads
    .map((d, i) => ({ d, i }))
    .filter((x) => x.d.status === "queued")
    .sort((a, b) => RANK[a.d.priority] - RANK[b.d.priority] || a.i - b.i);
  const position: Record<string, number> = {};
  queued.forEach((x, idx) => { position[x.d.id] = idx + 1; });

  return (
    <div className="bg-bg">
      <div className="flex gap-3 px-4 py-2 text-[10px] uppercase tracking-wide text-ink-mute border-b border-border">
        <div className="flex-[2]">Nom</div>
        <div className="flex-1">Cat.</div>
        <div className="flex-1">Priorité</div>
        <div className="flex-1">État</div>
        <div className="flex-1">Vitesse</div>
        <div className="w-24" />
      </div>
      {downloads.length === 0 ? (
        <div className="p-8 text-center text-ink-mute text-sm">Aucun téléchargement.</div>
      ) : (
        downloads.map((d) => (
          <DownloadRow
            key={d.id}
            d={d}
            speed={speeds[d.id] ?? 0}
            queuePosition={position[d.id] ?? null}
            seg={segments[d.id]}
            onPause={() => onPause(d.id)}
            onResume={() => onResume(d.id)}
            onCancel={() => onCancel(d.id)}
            onStop={() => onStop(d.id)}
            onPriority={(p) => onPriority(d.id, p)}
            onOpenFile={() => onOpenFile(d.id)}
            onOpenFolder={() => onOpenFolder(d.id)}
            onRemove={() => onRemove(d.id)}
          />
        ))
      )}
    </div>
  );
}
