import { useState } from "react";

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
      <button className="px-2 py-1 rounded bg-surface text-ink-soft hover:text-ink" onClick={() => setOpen((o) => !o)} title="Actions">⋯</button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-8 z-20 w-44 bg-surface border border-border rounded-lg shadow-[0_20px_50px_rgba(0,0,0,.5)] text-sm text-ink overflow-hidden">
            <button
              className="block w-full text-left px-3 py-2 hover:bg-bg disabled:opacity-40"
              disabled={!canOpen}
              onClick={() => { setOpen(false); onOpenFile(); }}
            >📂 Ouvrir le fichier</button>
            <button
              className="block w-full text-left px-3 py-2 hover:bg-bg disabled:opacity-40"
              disabled={!canOpen}
              onClick={() => { setOpen(false); onOpenFolder(); }}
            >🗂 Ouvrir le dossier</button>
            <button
              className="block w-full text-left px-3 py-2 hover:bg-bg border-t border-border"
              onClick={() => { setOpen(false); onRemove(); }}
            >🗑 Retirer de la liste</button>
          </div>
        </>
      )}
    </div>
  );
}
