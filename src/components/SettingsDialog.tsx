import { useEffect, useState } from "react";
import * as api from "../api";

export function SettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [maxConcurrent, setMaxConcurrent] = useState(3);
  const [speedValue, setSpeedValue] = useState(0); // en Mo/s dans l'UI
  const [unlimited, setUnlimited] = useState(true);

  useEffect(() => {
    if (!open) return;
    api.getSettings().then((s) => {
      setMaxConcurrent(s.max_concurrent);
      if (s.max_speed_bps === 0) {
        setUnlimited(true);
        setSpeedValue(0);
      } else {
        setUnlimited(false);
        setSpeedValue(Math.round((s.max_speed_bps / (1024 * 1024)) * 10) / 10);
      }
    });
  }, [open]);

  if (!open) return null;

  async function save() {
    const bps = unlimited ? 0 : Math.round(speedValue * 1024 * 1024);
    await api.updateSettings(Math.max(1, Math.round(maxConcurrent)), bps);
    onClose();
  }

  return (
    <div className="fixed inset-0 bg-black/30 flex items-center justify-center" onClick={onClose}>
      <div className="bg-white rounded-lg w-[380px]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-semibold px-4 py-3 border-b border-gray-200">⚙ Réglages</h3>
        <div className="px-4 py-3 border-b border-gray-100">
          <label className="block text-sm text-gray-600 mb-1.5">Téléchargements simultanés max</label>
          <input
            type="number"
            min={1}
            className="border border-gray-200 rounded-md px-2 py-1 text-sm w-24"
            value={maxConcurrent}
            onChange={(e) => setMaxConcurrent(Number(e.target.value))}
          />
        </div>
        <div className="px-4 py-3">
          <label className="block text-sm text-gray-600 mb-1.5">Vitesse de téléchargement max (Mo/s)</label>
          <div className="flex items-center gap-3">
            <input
              type="number"
              min={0}
              step={0.1}
              disabled={unlimited}
              className="border border-gray-200 rounded-md px-2 py-1 text-sm w-24 disabled:opacity-50"
              value={speedValue}
              onChange={(e) => setSpeedValue(Number(e.target.value))}
            />
            <label className="text-sm flex items-center gap-1">
              <input type="checkbox" checked={unlimited} onChange={(e) => setUnlimited(e.target.checked)} />
              Illimitée
            </label>
          </div>
        </div>
        <div className="flex justify-end gap-2 px-4 py-3">
          <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100" onClick={onClose}>Annuler</button>
          <button className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold" onClick={save}>
            Enregistrer
          </button>
        </div>
      </div>
    </div>
  );
}
