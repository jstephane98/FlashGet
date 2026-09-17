import { useEffect, useState } from "react";
import type { HlsVariant, MediaVariantsEvent } from "../api";

function fmtSize(bytes: number | null): string {
  if (bytes == null) return "—";
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) return `≈ ${(mb / 1024).toFixed(1)} Go`;
  return `≈ ${Math.round(mb)} Mo`;
}

function label(v: HlsVariant): string {
  if (v.resolution === "Audio uniquement") return "🎵 Audio uniquement (MP3)";
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
  onConfirm: (probeId: string, variantIndex: number, fileName: string) => void;
}) {
  const [selected, setSelected] = useState(0);
  const [name, setName] = useState("");
  useEffect(() => {
    if (event) {
      if (event.variants.length) {
        const best = event.variants.reduce((a, b) => (b.bandwidth > a.bandwidth ? b : a));
        setSelected(best.index);
      }
      setName(event.file_name);
    }
  }, [event]);
  if (!event) return null;
  return (
    <div className="fixed inset-0 bg-deep/80 flex items-center justify-center" onClick={onClose}>
      <div className="bg-bg border border-border rounded-xl p-5 w-[480px] text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-bold text-ink mb-1">Télécharger le média</h3>
        <div className="flex items-center gap-1 mb-3">
          <input
            className="flex-1 text-sm bg-surface text-ink placeholder:text-ink-mute border border-border rounded px-2 py-1"
            value={name}
            onChange={(e) => setName(e.target.value)}
            aria-label="Nom du fichier"
          />
          <span className="text-sm text-ink-mute shrink-0">
            {event.variants.find((v) => v.index === selected)?.resolution === "Audio uniquement" ? ".mp3" : ".mp4"}
          </span>
        </div>
        <div className="space-y-1 max-h-64 overflow-auto">
          {event.variants.map((v) => (
            <label
              key={v.index}
              className={
                "flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer bg-surface border " +
                (selected === v.index
                  ? "border-accent text-accent-bright"
                  : "border-border hover:border-accent/50")
              }
            >
              <input
                type="radio"
                name="variant"
                checked={selected === v.index}
                onChange={() => setSelected(v.index)}
              />
              <span className="text-sm font-mono">{label(v)}</span>
            </label>
          ))}
        </div>
        <div className="flex justify-end gap-2 mt-4">
          <button className="px-3 py-1.5 text-sm rounded-md bg-surface text-ink-soft border border-border" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md cta-grad font-semibold"
            onClick={() => onConfirm(event.probe_id, selected, name.trim())}
          >
            Télécharger
          </button>
        </div>
      </div>
    </div>
  );
}
