import { useEffect, useState } from "react";
import type { HlsVariant, MediaVariantsEvent } from "../api";

function fmtSize(bytes: number | null): string {
  if (!bytes) return "—";
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) return `≈ ${(mb / 1024).toFixed(1)} Go`;
  return `≈ ${Math.round(mb)} Mo`;
}

function label(v: HlsVariant): string {
  const res = v.resolution ? v.resolution.split("x")[1] + "p" : "audio/auto";
  const mbps = v.bandwidth ? `${(v.bandwidth / 1_000_000).toFixed(1)} Mbps` : "";
  return [res, mbps, fmtSize(v.est_size)].filter(Boolean).join(" · ");
}

export function HlsVariantDialog({
  event,
  onClose,
  onConfirm,
}: {
  event: MediaVariantsEvent | null;
  onClose: () => void;
  onConfirm: (probeId: string, variantIndex: number) => void;
}) {
  const [selected, setSelected] = useState(0);
  useEffect(() => {
    if (event && event.variants.length) {
      const best = event.variants.reduce((a, b) => (b.bandwidth > a.bandwidth ? b : a));
      setSelected(best.index);
    }
  }, [event]);
  if (!event) return null;
  return (
    <div className="fixed inset-0 bg-black/30 flex items-center justify-center" onClick={onClose}>
      <div className="bg-white rounded-lg p-5 w-[480px]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-semibold mb-1">Télécharger la vidéo</h3>
        <p className="text-sm text-gray-500 mb-3 truncate">{event.file_name}</p>
        <div className="space-y-1 max-h-64 overflow-auto">
          {event.variants.map((v) => (
            <label key={v.index} className="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-gray-50 cursor-pointer">
              <input
                type="radio"
                name="variant"
                checked={selected === v.index}
                onChange={() => setSelected(v.index)}
              />
              <span className="text-sm">{label(v)}</span>
            </label>
          ))}
        </div>
        <div className="flex justify-end gap-2 mt-4">
          <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold"
            onClick={() => onConfirm(event.probe_id, selected)}
          >
            Télécharger
          </button>
        </div>
      </div>
    </div>
  );
}
