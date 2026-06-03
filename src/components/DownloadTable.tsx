import type { Download, Priority } from "../types";
import { DownloadRow } from "./DownloadRow";

const RANK: Record<Priority, number> = { high: 0, normal: 1, low: 2 };

export function DownloadTable({
  downloads,
  speeds,
  onPause,
  onResume,
  onCancel,
  onPriority,
  onOpenFile,
  onOpenFolder,
  onRemove,
}: {
  downloads: Download[];
  speeds: Record<string, number>;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onPriority: (id: string, p: Priority) => void;
  onOpenFile: (id: string) => void;
  onOpenFolder: (id: string) => void;
  onRemove: (id: string) => void;
}) {
  const queued = downloads
    .map((d, i) => ({ d, i }))
    .filter((x) => x.d.status === "queued")
    .sort((a, b) => RANK[a.d.priority] - RANK[b.d.priority] || a.i - b.i);
  const position: Record<string, number> = {};
  queued.forEach((x, idx) => { position[x.d.id] = idx + 1; });

  return (
    <div>
      <div className="flex gap-3 px-3 py-2 text-[10px] uppercase tracking-wide text-gray-400 border-b border-gray-200 bg-gray-50">
        <div className="flex-[2]">Nom</div>
        <div className="flex-1">Cat.</div>
        <div className="flex-1">Priorité</div>
        <div className="flex-1">État</div>
        <div className="flex-1">Vitesse</div>
        <div className="w-24" />
      </div>
      {downloads.length === 0 ? (
        <div className="p-8 text-center text-gray-400 text-sm">Aucun téléchargement.</div>
      ) : (
        downloads.map((d) => (
          <DownloadRow
            key={d.id}
            d={d}
            speed={speeds[d.id] ?? 0}
            queuePosition={position[d.id] ?? null}
            onPause={() => onPause(d.id)}
            onResume={() => onResume(d.id)}
            onCancel={() => onCancel(d.id)}
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
