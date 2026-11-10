import { useEffect, useRef } from "react";
import { t } from "../../i18n";

/** Confirmation de suppression : retirer de la liste, ou aussi envoyer le fichier à la corbeille. */
export function DeleteDialog({ name, busy, error, onRemove, onDeleteFile, onCancel }: {
  name: string;
  busy: boolean;
  error: string | null;
  onRemove: () => void;
  onDeleteFile: () => void;
  onCancel: () => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => { cancelRef.current?.focus(); }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape" && !busy) onCancel(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel, busy]);

  return (
    <div className="fixed inset-0 bg-deep/80 flex items-center justify-center z-50" onClick={() => { if (!busy) onCancel(); }}>
      <div role="dialog" aria-modal="true" aria-labelledby="delete-dialog-title" className="bg-bg border border-border rounded-xl w-[420px] text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)]"
        onClick={(e) => e.stopPropagation()}>
        <h3 id="delete-dialog-title" className="font-bold px-4 py-3 border-b border-border break-words">{t("lib.deleteTitle", { name })}</h3>
        <p className="px-4 py-3 text-sm text-ink-soft">{t("lib.deleteHint")}</p>
        {error && <p role="alert" className="px-4 pb-2 text-sm text-err">{error}</p>}
        <div className="flex flex-wrap justify-end gap-2 px-4 py-3 border-t border-border">
          <button type="button" ref={cancelRef} disabled={busy} onClick={onCancel}
            className="px-3 py-1.5 rounded-md text-sm border border-line text-ink-soft hover:text-ink disabled:opacity-50">{t("lib.cancel")}</button>
          <button type="button" disabled={busy} onClick={onRemove}
            className="px-3 py-1.5 rounded-md text-sm border border-line bg-field text-ink hover:bg-surface disabled:opacity-50">{t("lib.removeOnly")}</button>
          <button type="button" disabled={busy} onClick={onDeleteFile}
            className="px-3 py-1.5 rounded-md text-sm border border-err/50 text-err hover:bg-err/10 disabled:opacity-50">{t("lib.deleteFile")}</button>
        </div>
      </div>
    </div>
  );
}
