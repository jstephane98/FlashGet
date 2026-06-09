import { Plus, ListPlus, Pause, Play, Search, Trash2, Settings } from "lucide-react";
import type { DownloadStatus } from "../types";

export function Toolbar({
  query,
  onQuery,
  status,
  onStatus,
  onAdd,
  onAddBatch,
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
  onAddBatch: () => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onClearCompleted: () => void;
  onSettings: () => void;
}) {
  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-border bg-surface">
      <button className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md cta-grad font-semibold" onClick={onAdd}><Plus size={16} /> Ajouter</button>
      <button className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md bg-bg text-ink font-semibold border border-border" onClick={onAddBatch}><ListPlus size={16} /> Lot</button>
      <button className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md bg-bg text-ink-soft border border-border" onClick={onPauseAll}><Pause size={16} /> Tout</button>
      <button className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md bg-bg text-ink-soft border border-border" onClick={onResumeAll}><Play size={16} /> Tout</button>
      <div className="relative flex-1">
        <Search size={15} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-ink-mute pointer-events-none" />
        <input
          className="w-full bg-bg text-ink placeholder:text-ink-mute border border-border rounded-md pl-8 pr-3 py-1.5 text-sm"
          placeholder="Rechercher par nom…"
          value={query}
          onChange={(e) => onQuery(e.target.value)}
        />
      </div>
      <select
        className="border border-border rounded-md px-2 py-1.5 text-sm bg-bg text-ink"
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
      <button className="px-3 py-1.5 text-sm rounded-md bg-bg text-ink-soft border border-border" onClick={onClearCompleted} title="Vider les terminés" aria-label="Vider les terminés"><Trash2 size={16} /></button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-bg text-ink-soft border border-border" onClick={onSettings} title="Réglages" aria-label="Réglages"><Settings size={16} /></button>
    </div>
  );
}
