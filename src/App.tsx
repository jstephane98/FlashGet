import { useEffect, useMemo, useRef, useState } from "react";
import "./index.css";
import { X } from "lucide-react";
import type { Download, Priority } from "./types";
import * as api from "./api";
import type { MediaVariantsEvent } from "./api";
import type { PageId } from "./navigation";
import { TitleBar } from "./components/shell/TitleBar";
import { Sidebar } from "./components/shell/Sidebar";
import { StatusBar } from "./components/shell/StatusBar";
import { DownloadsPage, type CardActions } from "./pages/DownloadsPage";
import { CompletedPage } from "./pages/CompletedPage";
import { AddUrlDialog } from "./components/AddUrlDialog";
import { AddBatchDialog } from "./components/AddBatchDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { HlsVariantDialog } from "./components/HlsVariantDialog";
import { ProbingOverlay } from "./components/ProbingOverlay";

const DEST_DIR = "";

export default function App() {
  const [page, setPage] = useState<PageId>("downloads");
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [parts, setParts] = useState<Record<string, number[] | null>>({});
  const [dialogOpen, setDialogOpen] = useState(false);
  const [batchOpen, setBatchOpen] = useState(false);
  const [info, setInfo] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mediaEvent, setMediaEvent] = useState<MediaVariantsEvent | null>(null);
  const [adding, setAdding] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);
  const addingRef = useRef(false);
  const probeTimeoutRef = useRef<number | null>(null);
  const [extProbing, setExtProbing] = useState(false);
  const extProbeTimeoutRef = useRef<number | null>(null);
  const [segs, setSegs] = useState<Record<string, { done: number; total: number }>>({});
  const [maxSpeedBps, setMaxSpeedBps] = useState(0);
  const [bridgeListening, setBridgeListening] = useState(false);

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

  // Données de la barre d'état : limite de vitesse et état du pont extension.
  function refreshStatusInfo() {
    api.getSettings().then((s) => setMaxSpeedBps(s.max_speed_bps)).catch(() => {});
    api.getBridgeInfo().then((b) => setBridgeListening(b.listening)).catch(() => {});
  }

  useEffect(() => {
    refresh();
    refreshStatusInfo();
    const un = api.onProgress((p) => {
      setSpeeds((s) => ({ ...s, [p.id]: p.speed_bps }));
      setParts((m) => ({ ...m, [p.id]: p.parts ?? null }));
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

  async function handleAddBatch(text: string) {
    setBatchOpen(false);
    setError(null);
    setInfo(null);
    try {
      const s = await api.addBatchDownloads(text, DEST_DIR);
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

  function openAddDialog() {
    stopProbing();
    setAddError(null);
    setDialogOpen(true);
  }

  // « Nouveau » et « Réglages » ouvrent encore les boîtes existantes (pages en phases 2 et 5) :
  // elles ne changent pas la page courante.
  function go(id: PageId) {
    if (id === "new") openAddDialog();
    else if (id === "settings") setSettingsOpen(true);
    else setPage(id);
  }

  const actions: CardActions = {
    pause: (id) => withRefresh(() => api.pauseDownload(id)),
    resume: (id) => withRefresh(() => api.startDownload(id)),
    cancel: (id) => withRefresh(() => api.cancelDownload(id)),
    stop: (id) => withRefresh(() => api.stopRecording(id)),
    priority: (id, p: Priority) => withRefresh(() => api.setPriority(id, p)),
    openFile: (id) => withRefresh(() => api.openFile(id)),
    openFolder: (id) => withRefresh(() => api.openFolder(id)),
    remove: (id) => withRefresh(() => api.removeFromList(id)),
  };

  const stats = useMemo(() => {
    let completed = 0, active = 0, queued = 0, totalSpeed = 0;
    for (const d of downloads) {
      if (d.status === "completed") completed++;
      else if (d.status === "queued") queued++;
      else if (d.status === "downloading" || d.status === "recording") {
        active++;
        totalSpeed += speeds[d.id] ?? 0;
      }
    }
    return { completed, pending: downloads.length - completed, active, queued, totalSpeed };
  }, [downloads, speeds]);

  return (
    <div className="h-screen flex flex-col bg-bg text-ink">
      <TitleBar />
      <div className="flex flex-1 min-h-0">
        <Sidebar
          active={page}
          counts={{ downloads: stats.pending, completed: stats.completed }}
          onNavigate={go}
        />
        <main className="flex-1 flex flex-col min-w-0">
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
          <div className="flex-1 min-h-0">
            {page === "completed" ? (
              <CompletedPage
                downloads={downloads}
                onClear={() => withRefresh(api.clearCompleted)}
                actions={actions}
              />
            ) : (
              <DownloadsPage
                downloads={downloads}
                speeds={speeds}
                parts={parts}
                segments={segs}
                onAddLink={openAddDialog}
                onAddBatch={() => setBatchOpen(true)}
                onPauseAll={() => withRefresh(api.pauseAll)}
                onResumeAll={() => withRefresh(api.resumeAll)}
                actions={actions}
              />
            )}
          </div>
          <StatusBar
            active={stats.active}
            queued={stats.queued}
            maxSpeedBps={maxSpeedBps}
            bridgeListening={bridgeListening}
            totalSpeedBps={stats.totalSpeed}
          />
        </main>
      </div>
      <AddUrlDialog
        open={dialogOpen}
        busy={adding}
        error={addError}
        onClose={() => { stopProbing(); setAddError(null); setDialogOpen(false); }}
        onAdd={handleAdd}
      />
      <AddBatchDialog open={batchOpen} onClose={() => setBatchOpen(false)} onAdd={handleAddBatch} />
      <SettingsDialog open={settingsOpen} onClose={() => { setSettingsOpen(false); refreshStatusInfo(); }} />
      <HlsVariantDialog event={mediaEvent} onClose={() => setMediaEvent(null)} onConfirm={handleHlsConfirm} />
      <ProbingOverlay open={extProbing} onClose={stopExtProbing} />
    </div>
  );
}
