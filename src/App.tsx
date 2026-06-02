import { useEffect, useState } from "react";
import "./index.css";
import type { Download } from "./types";
import * as api from "./api";
import { Toolbar } from "./components/Toolbar";
import { AddUrlDialog } from "./components/AddUrlDialog";
import { DownloadTable } from "./components/DownloadTable";

// Dossier de destination par défaut (Phase 1 : Téléchargements de l'utilisateur).
const DEST_DIR = ""; // chaîne vide => le backend pourra défaulter ; voir note plan.

export default function App() {
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [dialogOpen, setDialogOpen] = useState(false);

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
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  async function handleAdd(url: string) {
    setDialogOpen(false);
    const d = await api.addDownload(url, DEST_DIR);
    setDownloads((list) => [...list, d]);
    await api.startDownload(d.id);
  }

  return (
    <div className="h-screen flex flex-col">
      <Toolbar onAdd={() => setDialogOpen(true)} />
      <div className="flex-1 overflow-auto">
        <DownloadTable
          downloads={downloads}
          speeds={speeds}
          onPause={(id) => api.pauseDownload(id)}
          onResume={(id) => api.startDownload(id)}
          onCancel={(id) => {
            api.cancelDownload(id);
            setDownloads((l) => l.filter((d) => d.id !== id));
          }}
        />
      </div>
      <AddUrlDialog open={dialogOpen} onClose={() => setDialogOpen(false)} onAdd={handleAdd} />
    </div>
  );
}
