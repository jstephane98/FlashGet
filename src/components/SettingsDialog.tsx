import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import * as api from "../api";
import type { BridgeInfo, Category, CategoryDirs } from "../types";

const CATS: { key: Category; label: string }[] = [
  { key: "video", label: "🎬 Vidéos" },
  { key: "document", label: "📄 Documents" },
  { key: "program", label: "⚙ Programmes" },
  { key: "archive", label: "🗜 Archives" },
  { key: "music", label: "🎵 Musique" },
  { key: "other", label: "Autre" },
];

export function SettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [maxConcurrent, setMaxConcurrent] = useState(3);
  const [speedValue, setSpeedValue] = useState(0);
  const [unlimited, setUnlimited] = useState(true);
  const [dirs, setDirs] = useState<CategoryDirs | null>(null);
  const [bridge, setBridge] = useState<BridgeInfo | null>(null);
  const [copied, setCopied] = useState(false);
  const [autostart, setAutostart] = useState(false);

  useEffect(() => {
    if (!open) return;
    api.getSettings().then((s) => {
      setMaxConcurrent(s.max_concurrent);
      if (s.max_speed_bps === 0) { setUnlimited(true); setSpeedValue(0); }
      else { setUnlimited(false); setSpeedValue(Math.round((s.max_speed_bps / (1024 * 1024)) * 10) / 10); }
    });
    api.getCategoryDirs().then(setDirs);
    api.getBridgeInfo().then(setBridge);
    api.getAutostart().then(setAutostart);
  }, [open]);

  if (!open) return null;

  async function pickDir(cat: Category) {
    const selected = await openDialog({ directory: true });
    if (typeof selected === "string") {
      await api.setCategoryDir(cat, selected);
      setDirs(await api.getCategoryDirs());
    }
  }

  async function clearDir(cat: Category) {
    await api.setCategoryDir(cat, null);
    setDirs(await api.getCategoryDirs());
  }

  async function copyToken() {
    if (!bridge) return;
    await navigator.clipboard.writeText(bridge.token);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  async function regenToken() {
    const token = await api.regenerateBridgeToken();
    setBridge((b) => (b ? { ...b, token } : b));
  }

  async function save() {
    const bps = unlimited ? 0 : Math.round(speedValue * 1024 * 1024);
    await api.updateSettings(Math.max(1, Math.round(maxConcurrent)), bps);
    await api.setAutostart(autostart);
    onClose();
  }

  return (
    <div className="fixed inset-0 bg-black/30 flex items-center justify-center" onClick={onClose}>
      <div className="bg-white rounded-lg w-[460px] max-h-[80vh] overflow-auto" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-semibold px-4 py-3 border-b border-gray-200">⚙ Réglages</h3>
        <div className="px-4 py-3 border-b border-gray-100">
          <label className="block text-sm text-gray-600 mb-1.5">Téléchargements simultanés max</label>
          <input type="number" min={1} className="border border-gray-200 rounded-md px-2 py-1 text-sm w-24"
            value={maxConcurrent} onChange={(e) => setMaxConcurrent(Number(e.target.value))} />
        </div>
        <div className="px-4 py-3 border-b border-gray-100">
          <label className="block text-sm text-gray-600 mb-1.5">Vitesse de téléchargement max (Mo/s)</label>
          <div className="flex items-center gap-3">
            <input type="number" min={0} step={0.1} disabled={unlimited}
              className="border border-gray-200 rounded-md px-2 py-1 text-sm w-24 disabled:opacity-50"
              value={speedValue} onChange={(e) => setSpeedValue(Number(e.target.value))} />
            <label className="text-sm flex items-center gap-1">
              <input type="checkbox" checked={unlimited} onChange={(e) => setUnlimited(e.target.checked)} /> Illimitée
            </label>
          </div>
        </div>
        <div className="px-4 py-3 border-b border-gray-100">
          <label className="text-sm flex items-center gap-2">
            <input
              type="checkbox"
              checked={autostart}
              onChange={(e) => setAutostart(e.target.checked)}
            />
            Démarrer avec Windows (lancement caché dans la zone de notification)
          </label>
        </div>
        <div className="px-4 py-3">
          <div className="text-sm font-medium text-gray-700 mb-2">Dossiers par catégorie</div>
          <div className="text-[11px] text-gray-400 mb-2">Vide = dossier Téléchargements par défaut.</div>
          {CATS.map((c) => (
            <div key={c.key} className="flex items-center gap-2 mb-1.5 text-sm">
              <span className="w-28 text-gray-600">{c.label}</span>
              <span className="flex-1 truncate bg-gray-100 rounded px-2 py-1 text-xs text-gray-500">
                {(dirs && dirs[c.key]) || "(Téléchargements)"}
              </span>
              <button className="px-2 py-1 rounded bg-gray-100 text-xs" onClick={() => pickDir(c.key)}>Parcourir…</button>
              {dirs && dirs[c.key] && (
                <button className="px-2 py-1 rounded bg-gray-100 text-xs" onClick={() => clearDir(c.key)}>✕</button>
              )}
            </div>
          ))}
        </div>
        <div className="px-4 py-3 border-t border-gray-100">
          <div className="font-medium mb-2">🧩 Extension navigateur</div>
          <div className="flex items-center gap-2 mb-2 text-sm">
            <span
              className={
                "inline-block w-2 h-2 rounded-full " +
                (bridge?.listening ? "bg-green-500" : "bg-gray-400")
              }
            />
            <span className="text-gray-600">
              {bridge?.listening
                ? `Serveur local à l'écoute (port ${bridge.port})`
                : "Serveur local indisponible"}
            </span>
          </div>
          <label className="block text-xs text-gray-500 mb-1">
            Jeton d'appairage (à coller dans les options de l'extension)
          </label>
          <div className="flex items-center gap-2">
            <code className="flex-1 truncate bg-gray-100 rounded px-2 py-1 text-xs">
              {bridge?.token ?? "…"}
            </code>
            <button
              className="text-xs border rounded px-2 py-1 hover:bg-gray-50"
              onClick={copyToken}
            >
              {copied ? "Copié ✓" : "Copier"}
            </button>
            <button
              className="text-xs border rounded px-2 py-1 hover:bg-gray-50"
              onClick={regenToken}
            >
              Régénérer
            </button>
          </div>
        </div>
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-gray-100">
          <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100" onClick={onClose}>Fermer</button>
          <button className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold" onClick={save}>Enregistrer</button>
        </div>
      </div>
    </div>
  );
}
