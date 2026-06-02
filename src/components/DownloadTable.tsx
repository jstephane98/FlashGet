import type { Download } from "../types";
import { DownloadRow } from "./DownloadRow";

export function DownloadTable({
  downloads,
  speeds,
  onPause,
  onResume,
  onCancel,
}: {
  downloads: Download[];
  speeds: Record<string, number>;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
}) {
  return (
    <div>
      <div className="flex gap-3 px-3 py-2 text-[10px] uppercase tracking-wide text-gray-400 border-b border-gray-200 bg-gray-50">
        <div className="flex-[2]">Nom</div>
        <div className="flex-1">Taille</div>
        <div className="flex-1">État</div>
        <div className="flex-1">Vitesse</div>
        <div className="w-16" />
      </div>
      {downloads.length === 0 ? (
        <div className="p-8 text-center text-gray-400 text-sm">Aucun téléchargement.</div>
      ) : (
        downloads.map((d) => (
          <DownloadRow
            key={d.id}
            d={d}
            speed={speeds[d.id] ?? 0}
            onPause={() => onPause(d.id)}
            onResume={() => onResume(d.id)}
            onCancel={() => onCancel(d.id)}
          />
        ))
      )}
    </div>
  );
}
