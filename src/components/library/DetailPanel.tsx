import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Play, FolderOpen, Link, RotateCw, Trash2 } from "lucide-react";
import * as api from "../../api";
import type { Download } from "../../types";
import { formatBytes, formatElapsed } from "../../format";
import { formatLabel, downloadDuration, formatDay } from "../../library/library";
import { Thumb } from "./Thumb";
import { t } from "../../i18n";

type Player = { kind: "video" | "audio"; src: string } | null;

function folderOf(path: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i > 0 ? path.slice(0, i) : path;
}

export function DetailPanel({ d, onOpenFile, onOpenFolder, onCopyLink, onRedownload, onDelete }: {
  d: Download;
  onOpenFile: () => void;
  onOpenFolder: () => void;
  onCopyLink: () => void;
  onRedownload: () => void;
  onDelete: () => void;
}) {
  const [info, setInfo] = useState<api.FileInfo | null>(null);
  const [player, setPlayer] = useState<Player>(null);
  const [playError, setPlayError] = useState(false);

  useEffect(() => {
    let stale = false;
    setInfo(null);
    setPlayer(null);
    setPlayError(false);
    (async () => {
      let fi: api.FileInfo;
      try {
        fi = await api.getFileInfo(d.id);
      } catch {
        if (!stale) setInfo({ exists: false, size: 0, playable: null });
        return;
      }
      if (stale) return;
      setInfo(fi);
      if (fi.exists && fi.playable) {
        try {
          const path = await api.allowPlayback(d.id);
          if (stale) return;
          setPlayer({ kind: fi.playable, src: convertFileSrc(path) });
        } catch {
          if (!stale) setPlayError(true);
        }
      }
    })();
    return () => { stale = true; };
  }, [d.id]);

  const missing = info != null && !info.exists;
  const size = info?.exists && info.size != null && info.size > 0 ? info.size : d.total_bytes;
  const dur = downloadDuration(d);

  let top;
  if (missing) {
    top = (
      <div className="p-3 flex flex-col gap-2">
        <Thumb d={d} className="w-full h-32" iconSize={32} />
        <div className="text-sm text-err text-center">{t("lib.missing")}</div>
      </div>
    );
  } else if (playError) {
    top = <div className="p-4 text-sm text-ink-soft text-center">{t("lib.cantPlay")}</div>;
  } else if (player?.kind === "video") {
    top = <video key={player.src} controls src={player.src} onError={() => setPlayError(true)} className="w-full max-h-48 bg-black" />;
  } else if (player?.kind === "audio") {
    top = (
      <div className="p-3 flex flex-col gap-2">
        {d.thumbnail_url && <Thumb d={d} className="w-full h-32" iconSize={32} />}
        <audio key={player.src} controls src={player.src} onError={() => setPlayError(true)} className="w-full" />
      </div>
    );
  } else if (info && !info.playable) {
    top = <div className="p-3"><Thumb d={d} className="w-full h-32" iconSize={48} /></div>;
  } else {
    top = <div className="h-32" />;
  }

  const row = (label: string, value: React.ReactNode, cls = "") => (
    <>
      <dt className="text-ink-mute">{label}</dt>
      <dd className={`min-w-0 text-ink ${cls}`}>{value}</dd>
    </>
  );
  const btn = "flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-md text-sm border border-line bg-field text-ink hover:bg-surface disabled:opacity-50 disabled:cursor-not-allowed";

  return (
    <aside className="w-[320px] shrink-0 bg-panel border-l border-border flex flex-col overflow-y-auto">
      <div className="bg-surface border-b border-border">{top}</div>
      <div className="p-4 flex flex-col gap-3">
        <h2 className="font-bold text-ink text-sm line-clamp-2 break-words" title={d.file_name}>{d.file_name}</h2>
        <dl className="grid grid-cols-[64px_1fr] gap-x-3 gap-y-1.5 text-xs">
          {formatLabel(d) && row(t("lib.format"), formatLabel(d))}
          {row(t("lib.size"), size != null && size > 0 ? formatBytes(size) : "—", "font-mono")}
          {d.duration_s != null && row(t("lib.duration"), formatElapsed(d.duration_s), "font-mono")}
          {d.completed_at != null && row(t("lib.finished"), `${formatDay(d.completed_at)}${dur != null ? ` · ${formatElapsed(dur)}` : ""}`, "font-mono")}
          {row(t("lib.folder"), <span title={folderOf(d.dest_path)} className="block truncate font-mono text-[11px]" style={{ direction: "rtl", textAlign: "left" }}><bdi dir="ltr">{folderOf(d.dest_path)}</bdi></span>)}
          {row(t("lib.source"), <span title={d.url} className="block truncate text-accent-fg">{d.url}</span>)}
        </dl>
        <div className="grid grid-cols-2 gap-2">
          <button type="button" onClick={onOpenFile} disabled={missing}
            className="flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-md text-sm font-semibold bg-cta text-cta-ink hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed">
            <Play size={14} />{t("lib.open")}
          </button>
          <button type="button" onClick={onOpenFolder} className={btn}><FolderOpen size={14} />{t("lib.openFolder")}</button>
          <button type="button" onClick={onCopyLink} className={btn}><Link size={14} />{t("lib.copyLink")}</button>
          <button type="button" onClick={onRedownload} className={btn}><RotateCw size={14} />{t("lib.redownload")}</button>
        </div>
        <button type="button" onClick={onDelete}
          className="w-full flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-md text-sm border border-err/40 text-err hover:bg-err/10">
          <Trash2 size={14} />{t("lib.delete")}
        </button>
      </div>
    </aside>
  );
}
