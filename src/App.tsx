import { useEffect, useState } from "react";
import "./index.css";
import type { Download, Priority, Category, DownloadStatus } from "./types";
import * as api from "./api";
import { filterDownloads } from "./filter";
import { Toolbar } from "./components/Toolbar";
import { CategoryTabs } from "./components/CategoryTabs";
import { AddUrlDialog } from "./components/AddUrlDialog";
import { DownloadTable } from "./components/DownloadTable";
import { SettingsDialog } from "./components/SettingsDialog";
import { HlsVariantDialog } from "./components/HlsVariantDialog";
import type { MediaVariantsEvent } from "./api";

const DEST_DIR = "";

export default function App() {
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [dialogOpen, setDialogOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<Category | "all">("all");
  const [status, setStatus] = useState<DownloadStatus | "all">("all");
  const [mediaEvent, setMediaEvent] = useState<MediaVariantsEvent | null>(null);
  const [segs, setSegs] = useState<Record<string, { done: number; total: number }>>({});

  async function refresh() {
    setDownloads(await api.listDownloads());
  }

  useEffect(() => {
    refresh();
    const un = api.onProgress((p) => {
      setSpeeds((s) => ({ ...s, [p.id]: p.speed_bps }));
      if (p.segments_total != null && p.segments_done != null) {
        setSegs((m) => ({ ...m, [p.id]: { done: p.segments_done!, total: p.segments_total! } }));
      }
      setDownloads((list) =>
        list.map((d) =>
          d.id === p.id ? { ...d, downloaded_bytes: p.downloaded_bytes, status: p.status, total_bytes: p.total_bytes } : d,
        ),
      );
      if (p.status === "completed" || p.status === "paused" || p.status === "error") refresh();
    });
    // Un download créé hors UI (capture par l'extension via le bridge) : rafraîchir la liste.
    const unAdded = api.onDownloadAdded(() => refresh());
    const unMedia = api.onMediaVariants((e) => setMediaEvent(e));
    const unMediaErr = api.onMediaError((e) => setError(`Vidéo : ${e.message}`));
    return () => { un.then((f) => f()); unAdded.then((f) => f()); unMedia.then((f) => f()); unMediaErr.then((f) => f()); };
  }, []);

  async function handleHlsConfirm(probeId: string, variantIndex: number) {
    setMediaEvent(null);
    try {
      await api.addHlsDownload(probeId, variantIndex);
      await refresh();
    } catch (e) {
      setError(`Impossible de démarrer la vidéo : ${String(e)}`);
    }
  }

  async function handleAdd(url: string) {
    setDialogOpen(false);
    setError(null);
    try {
      await api.addDownload(url, DEST_DIR);
      await refresh();
    } catch (e) {
      setError(`Impossible d'ajouter ce lien : ${String(e)}`);
    }
  }

  async function withRefresh(fn: () => Promise<void>) {
    try { await fn(); } catch (e) { setError(String(e)); }
    await refresh();
  }

  const visible = filterDownloads(downloads, { query, category, status });

  return (
    <div className="h-screen flex flex-col">
      <Toolbar
        query={query} onQuery={setQuery}
        status={status} onStatus={setStatus}
        onAdd={() => setDialogOpen(true)}
        onPauseAll={() => withRefresh(api.pauseAll)}
        onResumeAll={() => withRefresh(api.resumeAll)}
        onClearCompleted={() => withRefresh(api.clearCompleted)}
        onSettings={() => setSettingsOpen(true)}
      />
      <CategoryTabs active={category} onChange={setCategory} />
      {error && (
        <div className="flex items-start gap-2 px-3 py-2 bg-red-50 border-b border-red-200 text-sm text-red-700">
          <span className="flex-1 break-words">{error}</span>
          <button className="px-2 py-0.5 rounded bg-red-100 text-red-700 shrink-0" onClick={() => setError(null)}>✕</button>
        </div>
      )}
      <div className="flex-1 overflow-auto">
        <DownloadTable
          downloads={visible}
          allDownloads={downloads}
          speeds={speeds}
          segments={segs}
          onPause={(id) => withRefresh(() => api.pauseDownload(id))}
          onResume={(id) => withRefresh(() => api.startDownload(id))}
          onCancel={(id) => withRefresh(() => api.cancelDownload(id))}
          onPriority={(id, p: Priority) => withRefresh(() => api.setPriority(id, p))}
          onOpenFile={(id) => withRefresh(() => api.openFile(id))}
          onOpenFolder={(id) => withRefresh(() => api.openFolder(id))}
          onRemove={(id) => withRefresh(() => api.removeFromList(id))}
        />
      </div>
      <AddUrlDialog open={dialogOpen} onClose={() => setDialogOpen(false)} onAdd={handleAdd} />
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      <HlsVariantDialog event={mediaEvent} onClose={() => setMediaEvent(null)} onConfirm={handleHlsConfirm} />
    </div>
  );
}
