import { useState } from "react";

export function AddUrlDialog({
  open,
  onClose,
  onAdd,
}: {
  open: boolean;
  onClose: () => void;
  onAdd: (url: string) => void;
}) {
  const [url, setUrl] = useState("");
  if (!open) return null;
  return (
    <div className="fixed inset-0 bg-black/30 flex items-center justify-center" onClick={onClose}>
      <div className="bg-white rounded-lg p-5 w-[480px]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-semibold mb-3">Nouveau téléchargement</h3>
        <input
          autoFocus
          className="w-full border border-gray-200 rounded-md px-3 py-2 text-sm"
          placeholder="Colle une URL…"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && url.trim()) {
              onAdd(url.trim());
              setUrl("");
            }
          }}
        />
        <div className="flex justify-end gap-2 mt-4">
          <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold disabled:opacity-50"
            disabled={!url.trim()}
            onClick={() => {
              onAdd(url.trim());
              setUrl("");
            }}
          >
            Ajouter
          </button>
        </div>
      </div>
    </div>
  );
}
