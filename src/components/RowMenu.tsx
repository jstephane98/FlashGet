import { useState } from "react";
import { MoreHorizontal, File, FolderOpen, Trash2 } from "lucide-react";

export function RowMenu({
  canOpen,
  onOpenFile,
  onOpenFolder,
  onRemove,
}: {
  canOpen: boolean;
  onOpenFile: () => void;
  onOpenFolder: () => void;
  onRemove: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative">
      <button className="inline-flex items-center justify-center px-2 py-1 rounded bg-surface text-ink-soft hover:text-ink" onClick={() => setOpen((o) => !o)} title="Actions" aria-label="Actions"><MoreHorizontal size={16} /></button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-8 z-20 w-44 bg-surface border border-border rounded-lg shadow-[0_20px_50px_rgba(0,0,0,.5)] text-sm text-ink overflow-hidden">
            <button
              className="block w-full text-left px-3 py-2 hover:bg-bg disabled:opacity-40"
              disabled={!canOpen}
              onClick={() => { setOpen(false); onOpenFile(); }}
            ><span className="inline-flex items-center gap-2"><File size={15} /> Ouvrir le fichier</span></button>
            <button
              className="block w-full text-left px-3 py-2 hover:bg-bg disabled:opacity-40"
              disabled={!canOpen}
              onClick={() => { setOpen(false); onOpenFolder(); }}
            ><span className="inline-flex items-center gap-2"><FolderOpen size={15} /> Ouvrir le dossier</span></button>
            <button
              className="block w-full text-left px-3 py-2 hover:bg-bg border-t border-border"
              onClick={() => { setOpen(false); onRemove(); }}
            ><span className="inline-flex items-center gap-2"><Trash2 size={15} /> Retirer de la liste</span></button>
          </div>
        </>
      )}
    </div>
  );
}
