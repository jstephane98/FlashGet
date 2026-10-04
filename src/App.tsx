import { useEffect, useRef, useState } from "react";
import "./index.css";
import { X } from "lucide-react";
import type { Download, Priority, Category, DownloadStatus } from "./types";
import * as api from "./api";
import { filterDownloads } from "./filter";
import { Toolbar } from "./components/Toolbar";
import { CategoryTabs } from "./components/CategoryTabs";
import { AddUrlDialog } from "./components/AddUrlDialog";
import { AddBatchDialog, type BatchMode } from "./components/AddBatchDialog";
import { DownloadTable } from "./components/DownloadTable";
import { SettingsDialog } from "./components/SettingsDialog";
import { HlsVariantDialog } from "./components/HlsVariantDialog";
import { ProbingOverlay } from "./components/ProbingOverlay";
import type { MediaVariantsEvent } from "./api";

const DEST_DIR = "";

export default function App() {
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [dialogOpen, setDialogOpen] = useState(false);
  const [batchOpen, setBatchOpen] = useState(false);
  const [info, setInfo] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<Category | "all">("all");
  const [status, setStatus] = useState<DownloadStatus | "all">("all");
  const [mediaEvent, setMediaEvent] = useState<MediaVariantsEvent | null>(null);
  const [adding, setAdding] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);
  const addingRef = useRef(false);
  const probeTimeoutRef = useRef<number | null>(null);
  const [extProbing, setExtProbing] = useState(false);
  const extProbeTimeoutRef = useRef<number | null>(null);
  const [segs, setSegs] = useState<Record<string, { done: number; total: number }>>({});

  function stopProbing() {
    setAdding(false);
    addingRef.current = false;
    if (probeTimeoutRef.current != null) {
      clearTimeout(probeTimeoutRef.current);
      probeTimeoutRef.current = null;
    }
  }

  function stopExtProbing() {
    setExtProbing(false);
    if (extProbeTimeoutRef.current != null) {
      clearTimeout(extProbeTimeoutRef.current);
      extProbeTimeoutRef.current = null;
    }
  }

  async function refresh() {
    setDownloads(await api.listDownloads());
  }

  useEffect(() => {
    refresh();
    const un = api.onProgress((p) => {
      setSpeeds((s) => ({ ...s, [p.id]: p.speed_bps }));
      // Live : segments_done croît mais segments_total est null (total inconnu) ;
      // on stocke quand même la progression segments (total 0 = indéterminé).
      if (p.segments_done != null) {
        setSegs((m) => ({ ...m, [p.id]: { done: p.segments_done!, total: p.segments_total ?? 0 } }));
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
    const unProbing = api.onMediaProbing(() => {
      setExtProbing(true);
      if (extProbeTimeoutRef.current != null) clearTimeout(extProbeTimeoutRef.current);
      extProbeTimeoutRef.current = window.setTimeout(() => {
        setExtProbing(false);
        extProbeTimeoutRef.current = null;
        setError("Délai dépassé — la recherche des formats n'a pas abouti.");
      }, 60000);
    });
    const unMedia = api.onMediaVariants((e) => {
      stopExtProbing();
      stopProbing();
      setAddError(null);
      setDialogOpen(false);   // ferme AddUrlDialog (sans effet si déjà fermée — cas extension)
      setMediaEvent(e);       // ouvre HlsVariantDialog, déjà rempli
    });
    const unMediaErr = api.onMediaError((e) => {
      stopExtProbing();
      const fromDialog = addingRef.current;  // lire AVANT stopProbing (qui le remet à false)
      stopProbing();
      if (fromDialog) setAddError(`Vidéo : ${e.message}`);
      else setError(`Vidéo : ${e.message}`);
    });
    return () => {
      un.then((f) => f()); unAdded.then((f) => f()); unMedia.then((f) => f()); unMediaErr.then((f) => f()); unProbing.then((f) => f());
      if (probeTimeoutRef.current != null) clearTimeout(probeTimeoutRef.current);
      if (extProbeTimeoutRef.current != null) clearTimeout(extProbeTimeoutRef.current);
    };
  }, []);

  async function handleHlsConfirm(probeId: string, variantIndex: number, fileName: string, timeRange?: string) {
    setMediaEvent(null);
    try {
      await api.addMediaDownload(probeId, variantIndex, fileName, timeRange);
      refresh();
    } catch (e) {
      setError(`Impossible de démarrer la vidéo : ${String(e)}`);
    }
  }

  async function handleAdd(url: string) {
    stopProbing(); // annule tout timer/sonde précédent avant de ré-armer
    // La boîte reste ouverte : spinner pendant la sonde.
    setAdding(true);
    addingRef.current = true;
    setAddError(null);
    setError(null);
    setInfo(null);
    try {
      await api.addDownload(url, DEST_DIR);
      // Fichier normal : la sonde a réussi sans variantes → on ferme et on rafraîchit.
      stopProbing();
      setDialogOpen(false);
      await refresh();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("__media_pending__")) {
        // Sonde média en tâche de fond : le spinner continue jusqu'à media://variants
        // ou media://error. Filet de sécurité : timeout 60 s.
        probeTimeoutRef.current = window.setTimeout(() => {
          stopProbing();
          setAddError("Délai dépassé — réessaie.");
        }, 60000);
      } else {
        stopProbing();
        setAddError(`Impossible d'ajouter ce lien : ${msg}`);
      }
    }
  }

  async function handleAddBatch(text: string, mode: BatchMode) {
    setBatchOpen(false);
    setError(null);
    setInfo(mode === "direct" ? null : "Lecture des liens/playlists en cours…");
    try {
      const s = await api.addBatchDownloads(text, DEST_DIR, mode);
      await refresh();
      const parts = [`${s.added} ajouté(s)`];
      if (s.skipped_duplicate) parts.push(`${s.skipped_duplicate} doublon(s)`);
      if (s.skipped_invalid) parts.push(`${s.skipped_invalid} ignoré(s)`);
      if (s.failed) parts.push(`${s.failed} échec(s)`);
      setInfo(parts.join(" · "));
    } catch (e) {
      setError(`Échec de l'ajout en lot : ${String(e)}`);
    }
  }

  async function withRefresh(fn: () => Promise<void>) {
    try { await fn(); } catch (e) { setError(String(e)); }
    await refresh();
  }

  const visible = filterDownloads(downloads, { query, category, status });

  return (
    <div className="h-screen flex flex-col bg-bg text-ink">
      <Toolbar
        query={query} onQuery={setQuery}
        status={status} onStatus={setStatus}
        onAdd={() => { stopProbing(); setAddError(null); setDialogOpen(true); }}
        onAddBatch={() => setBatchOpen(true)}
        onPauseAll={() => withRefresh(api.pauseAll)}
        onResumeAll={() => withRefresh(api.resumeAll)}
        onClearCompleted={() => withRefresh(api.clearCompleted)}
        onSettings={() => setSettingsOpen(true)}
      />
      <CategoryTabs active={category} onChange={setCategory} />
      {error && (
        <div className="flex items-start gap-2 px-3 py-2 bg-err/10 border-b border-err/40 text-sm text-err">
          <span className="flex-1 break-words">{error}</span>
          <button className="px-2 py-0.5 rounded bg-err/20 text-err shrink-0" onClick={() => setError(null)} aria-label="Fermer"><X size={14} /></button>
        </div>
      )}
      {info && (
        <div className="flex items-start gap-2 px-3 py-2 bg-accent/10 border-b border-accent/40 text-sm text-accent-bright">
          <span className="flex-1 break-words">{info}</span>
          <button className="px-2 py-0.5 rounded bg-accent/20 text-accent-bright shrink-0" onClick={() => setInfo(null)} aria-label="Fermer"><X size={14} /></button>
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
          onStop={(id) => withRefresh(() => api.stopRecording(id))}
          onPriority={(id, p: Priority) => withRefresh(() => api.setPriority(id, p))}
          onOpenFile={(id) => withRefresh(() => api.openFile(id))}
          onOpenFolder={(id) => withRefresh(() => api.openFolder(id))}
          onRemove={(id) => withRefresh(() => api.removeFromList(id))}
        />
      </div>
      <AddUrlDialog
        open={dialogOpen}
        busy={adding}
        error={addError}
        onClose={() => { stopProbing(); setAddError(null); setDialogOpen(false); }}
        onAdd={handleAdd}
      />
      <AddBatchDialog open={batchOpen} onClose={() => setBatchOpen(false)} onAdd={handleAddBatch} />
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      <HlsVariantDialog event={mediaEvent} onClose={() => setMediaEvent(null)} onConfirm={handleHlsConfirm} />
      <ProbingOverlay open={extProbing} onClose={stopExtProbing} />
    </div>
  );
}
