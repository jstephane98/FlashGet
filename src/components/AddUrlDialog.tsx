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
    <div className="fixed inset-0 bg-deep/80 flex items-center justify-center" onClick={onClose}>
      <div className="bg-bg border border-border rounded-xl p-5 w-[480px] text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-bold mb-3 text-ink">Nouveau téléchargement</h3>
        <input
          autoFocus
          className="w-full bg-surface text-ink placeholder:text-ink-mute border border-border rounded-md px-3 py-2 text-sm"
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
          <button className="px-3 py-1.5 text-sm rounded-md bg-surface text-ink-soft border border-border" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md cta-grad font-semibold disabled:opacity-50"
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
