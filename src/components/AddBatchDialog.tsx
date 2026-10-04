import { useState } from "react";

export type BatchMode = "direct" | "video" | "audio";

export function AddBatchDialog({
  open,
  onClose,
  onAdd,
}: {
  open: boolean;
  onClose: () => void;
  onAdd: (text: string, mode: BatchMode) => void;
}) {
  const [text, setText] = useState("");
  const [mode, setMode] = useState<BatchMode>("direct");
  if (!open) return null;

  function submit() {
    if (!text.trim()) return;
    onAdd(text, mode);
    setText("");
  }

  return (
    <div className="fixed inset-0 bg-deep/80 flex items-center justify-center" onClick={onClose}>
      <div className="bg-bg border border-border rounded-xl p-5 w-[560px] text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-bold mb-1 text-ink">Ajouter plusieurs liens</h3>
        <p className="text-xs text-ink-soft mb-3">
          Un lien par ligne. Pour YouTube, collez un lien de playlist : toutes ses vidéos seront ajoutées.
        </p>
        <div className="flex items-center gap-2 mb-3 text-sm">
          <span className="text-ink-mute">Format de sortie :</span>
          <select
            aria-label="Format de sortie"
            className="bg-surface border border-border rounded px-2 py-1 text-ink"
            value={mode}
            onChange={(e) => setMode(e.target.value as BatchMode)}
          >
            <option value="direct">Fichiers directs</option>
            <option value="video">Vidéo (MP4, meilleure qualité)</option>
            <option value="audio">Audio uniquement (MP3)</option>
          </select>
        </div>
        <textarea
          autoFocus
          aria-label="Liens à télécharger"
          className="w-full h-48 bg-surface text-ink placeholder:text-ink-mute border border-border rounded-md px-3 py-2 text-sm font-mono resize-none"
          placeholder={"https://exemple.com/fichier1.zip\nhttps://www.youtube.com/playlist?list=..."}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) submit();
          }}
        />
        <div className="flex justify-end gap-2 mt-4">
          <button className="px-3 py-1.5 text-sm rounded-md bg-surface text-ink-soft border border-border" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md cta-grad font-semibold disabled:opacity-50"
            disabled={!text.trim()}
            onClick={submit}
          >
            Ajouter
          </button>
        </div>
      </div>
    </div>
  );
}
