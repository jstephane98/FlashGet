import { useEffect, useState } from "react";
import "./index.css";
import type { Download, Priority } from "./types";
import * as api from "./api";
import { Toolbar } from "./components/Toolbar";
import { AddUrlDialog } from "./components/AddUrlDialog";
import { DownloadTable } from "./components/DownloadTable";
import { SettingsDialog } from "./components/SettingsDialog";

const DEST_DIR = ""; // vide => le backend défaut sur le dossier Téléchargements

export default function App() {
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [dialogOpen, setDialogOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    setDownloads(await api.listDownloads());
  }

  useEffect(() => {
    refresh();
    const un = api.onProgress((p) => {
      setSpeeds((s) => ({ ...s, [p.id]: p.speed_bps }));
      setDownloads((list) =>
        list.map((d) =>
          d.id === p.id
            ? { ...d, downloaded_bytes: p.downloaded_bytes, status: p.status, total_bytes: p.total_bytes }
            : d,
        ),
      );
      // Un changement d'état terminal peut avoir démarré un autre téléchargement : resync.
      if (p.status === "completed" || p.status === "paused" || p.status === "error") {
        refresh();
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  async function handleAdd(url: string) {
    setDialogOpen(false);
    setError(null);
    try {
      await api.addDownload(url, DEST_DIR);
      await refresh(); // le backend démarre automatiquement si un slot est libre
    } catch (e) {
      setError(`Impossible d'ajouter ce lien : ${String(e)}`);
    }
  }

  async function withRefresh(fn: () => Promise<void>) {
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    }
    await refresh();
  }

  return (
    <div className="h-screen flex flex-col">
      <Toolbar
        onAdd={() => setDialogOpen(true)}
        onPauseAll={() => withRefresh(api.pauseAll)}
        onResumeAll={() => withRefresh(api.resumeAll)}
        onSettings={() => setSettingsOpen(true)}
      />
      {error && (
        <div className="flex items-start gap-2 px-3 py-2 bg-red-50 border-b border-red-200 text-sm text-red-700">
          <span className="flex-1 break-words">{error}</span>
          <button className="px-2 py-0.5 rounded bg-red-100 text-red-700 shrink-0" onClick={() => setError(null)}>✕</button>
        </div>
      )}
      <div className="flex-1 overflow-auto">
        <DownloadTable
          downloads={downloads}
          speeds={speeds}
          onPause={(id) => withRefresh(() => api.pauseDownload(id))}
          onResume={(id) => withRefresh(() => api.startDownload(id))}
          onCancel={(id) => {
            api.cancelDownload(id);
            setDownloads((l) => l.filter((d) => d.id !== id));
          }}
          onPriority={(id, p: Priority) => withRefresh(() => api.setPriority(id, p))}
        />
      </div>
      <AddUrlDialog open={dialogOpen} onClose={() => setDialogOpen(false)} onAdd={handleAdd} />
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}
