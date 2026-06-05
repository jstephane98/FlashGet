import type { DownloadStatus } from "../types";

export function Toolbar({
  query,
  onQuery,
  status,
  onStatus,
  onAdd,
  onPauseAll,
  onResumeAll,
  onClearCompleted,
  onSettings,
}: {
  query: string;
  onQuery: (q: string) => void;
  status: DownloadStatus | "all";
  onStatus: (s: DownloadStatus | "all") => void;
  onAdd: () => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onClearCompleted: () => void;
  onSettings: () => void;
}) {
  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-200 bg-white">
      <button className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold" onClick={onAdd}>+ Ajouter</button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onPauseAll}>⏸ Tout</button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onResumeAll}>▶ Tout</button>
      <input
        className="flex-1 bg-gray-100 rounded-md px-3 py-1.5 text-sm"
        placeholder="🔍 Rechercher par nom…"
        value={query}
        onChange={(e) => onQuery(e.target.value)}
      />
      <select
        className="border border-gray-200 rounded-md px-2 py-1.5 text-sm bg-white"
        value={status}
        onChange={(e) => onStatus(e.target.value as DownloadStatus | "all")}
      >
        <option value="all">Tous les statuts</option>
        <option value="downloading">En cours</option>
        <option value="recording">Enregistrement</option>
        <option value="queued">En file</option>
        <option value="paused">En pause</option>
        <option value="completed">Terminé</option>
        <option value="error">Erreur</option>
      </select>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onClearCompleted} title="Vider les terminés">🧹</button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onSettings} title="Réglages">⚙</button>
    </div>
  );
}
