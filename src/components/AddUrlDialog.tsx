import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";

export function AddUrlDialog({
  open,
  busy,
  error,
  onClose,
  onAdd,
}: {
  open: boolean;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onAdd: (url: string) => void;
}) {
  const [url, setUrl] = useState("");

  // Reset du champ à chaque ouverture fraîche de la boîte.
  useEffect(() => {
    if (open) setUrl("");
  }, [open]);

  if (!open) return null;

  const canSubmit = !busy && url.trim().length > 0;
  function submit() {
    if (!canSubmit) return;
    onAdd(url.trim());
    // On NE vide PAS `url` : en cas d'erreur inline, l'utilisateur peut corriger/réessayer.
  }

  return (
    <div className="fixed inset-0 bg-deep/80 flex items-center justify-center" onClick={onClose}>
      <div className="bg-bg border border-border rounded-xl p-5 w-[480px] text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-bold mb-3 text-ink">Nouveau téléchargement</h3>
        <input
          autoFocus
          className="w-full bg-surface text-ink placeholder:text-ink-mute border border-border rounded-md px-3 py-2 text-sm disabled:opacity-50"
          placeholder="Colle une URL…"
          value={url}
          disabled={busy}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
        />
        {busy && (
          <div className="flex items-center gap-2 mt-3 text-sm text-ink-soft">
            <Loader2 size={16} className="animate-spin" aria-hidden="true" />
            Recherche des formats disponibles…
          </div>
        )}
        {error && (
          <div className="mt-3 text-sm text-err break-words">{error}</div>
        )}
        <div className="flex justify-end gap-2 mt-4">
          <button className="px-3 py-1.5 text-sm rounded-md bg-surface text-ink-soft border border-border" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md cta-grad font-semibold disabled:opacity-50 inline-flex items-center gap-2"
            disabled={!canSubmit}
            onClick={submit}
          >
            {busy && <Loader2 size={14} className="animate-spin" aria-hidden="true" />}
            Ajouter
          </button>
        </div>
      </div>
    </div>
  );
}
