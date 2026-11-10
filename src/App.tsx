import { useEffect, useMemo, useRef, useState } from "react";
import "./index.css";
import { X } from "lucide-react";
import type { Download, Priority } from "./types";
import * as api from "./api";
import type { PageId } from "./navigation";
import { TitleBar } from "./components/shell/TitleBar";
import { Sidebar } from "./components/shell/Sidebar";
import { StatusBar } from "./components/shell/StatusBar";
import { DownloadsPage, type CardActions } from "./pages/DownloadsPage";
import { CompletedPage } from "./pages/CompletedPage";
import { NewPage, type ExternalEvent } from "./pages/NewPage";
import { SettingsDialog } from "./components/SettingsDialog";

type ExternalPayload =
  | { kind: "probing"; url: string }
  | { kind: "probed"; probe: api.LinkProbe }
  | { kind: "error"; url: string; message: string };

export default function App() {
  const [page, setPage] = useState<PageId>("downloads");
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [parts, setParts] = useState<Record<string, number[] | null>>({});
  const [info, setInfo] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [external, setExternal] = useState<ExternalEvent | null>(null);
  const [startInBatch, setStartInBatch] = useState(false);
  const extSeq = useRef(0);
  const [segs, setSegs] = useState<Record<string, { done: number; total: number }>>({});
  const [maxSpeedBps, setMaxSpeedBps] = useState(0);
  const [bridgeListening, setBridgeListening] = useState(false);

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
    // Liens envoyés par l'extension : la page Nouveau s'ouvre, préremplie ; rien ne démarre
    // avant confirmation. La fenêtre est déjà mise au premier plan côté Rust.
    const showExternal = (e: ExternalPayload) => {
      extSeq.current += 1;
      setExternal({ ...e, seq: extSeq.current });
      setStartInBatch(false);
      setPage("new");
    };
    const unProbing = api.onLinkProbing(({ url }) => showExternal({ kind: "probing", url }));
    const unProbed = api.onLinkProbed((probe) => showExternal({ kind: "probed", probe }));
    const unLinkErr = api.onLinkError(({ url, message }) => showExternal({ kind: "error", url, message }));
    return () => {
      un.then((f) => f());
      unProbing.then((f) => f()); unProbed.then((f) => f()); unLinkErr.then((f) => f());
    };
  }, []);

  async function withRefresh(fn: () => Promise<void>) {
    try { await fn(); } catch (e) { setError(String(e)); }
    await refresh();
  }

  function openNew(batch: boolean) {
    // Un événement extension déjà traité ne doit pas être rejoué au remontage de la page.
    setExternal(null);
    setStartInBatch(batch);
    setPage("new");
  }

  // « Réglages » ouvre encore la boîte existante (page en phase 5) : elle ne change pas la page courante.
  function go(id: PageId) {
    if (id === "new") openNew(false);
    else if (id === "settings") setSettingsOpen(true);
    else setPage(id);
  }

  function handleNewDone(msg?: string) {
    setExternal(null);
    setStartInBatch(false);
    setPage("downloads");
    refresh();
    if (msg) setInfo(msg);
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
      else if (d.status === "queued" || d.status === "held") queued++;
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
            {page === "new" ? (
              <NewPage external={external} startInBatch={startInBatch} onDone={handleNewDone} />
            ) : page === "completed" ? (
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
                onAddLink={() => openNew(false)}
                onAddBatch={() => openNew(true)}
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
      <SettingsDialog open={settingsOpen} onClose={() => { setSettingsOpen(false); refreshStatusInfo(); }} />
    </div>
  );
}
