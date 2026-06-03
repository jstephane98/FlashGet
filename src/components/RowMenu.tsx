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
      <button className="px-2 py-1 rounded bg-gray-100" onClick={() => setOpen((o) => !o)} title="Actions">⋯</button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-8 z-20 w-44 bg-white border border-gray-200 rounded-lg shadow-lg text-sm overflow-hidden">
            <button
              className="block w-full text-left px-3 py-2 hover:bg-gray-50 disabled:opacity-40"
              disabled={!canOpen}
              onClick={() => { setOpen(false); onOpenFile(); }}
            >📂 Ouvrir le fichier</button>
            <button
              className="block w-full text-left px-3 py-2 hover:bg-gray-50 disabled:opacity-40"
              disabled={!canOpen}
              onClick={() => { setOpen(false); onOpenFolder(); }}
            >🗂 Ouvrir le dossier</button>
            <button
              className="block w-full text-left px-3 py-2 hover:bg-gray-50 border-t border-gray-100"
              onClick={() => { setOpen(false); onRemove(); }}
            >🗑 Retirer de la liste</button>
          </div>
        </>
      )}
    </div>
  );
}
