import { useEffect, useRef, useState } from "react";
import { t } from "../i18n";
import type { EndAction } from "../api";

const SECONDS = 60;

/** Compte à rebours avant l'action de fin de file : « Annuler » l'empêche, « Maintenant » la lance tout de suite. */
export function EndOfQueueDialog({ action, onCancel, onRun }: {
  action: EndAction;
  onCancel: () => void;
  onRun: () => void;
}) {
  const [s, setS] = useState(SECONDS);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const onRunRef = useRef(onRun);
  onRunRef.current = onRun;
  const firedRef = useRef(false);

  useEffect(() => { cancelRef.current?.focus(); }, []);
  useEffect(() => {
    const id = setInterval(() => setS((v) => Math.max(0, v - 1)), 1000);
    return () => clearInterval(id);
  }, []);
  useEffect(() => {
    if (s === 0 && !firedRef.current) {
      firedRef.current = true;
      onRunRef.current();
    }
  }, [s]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onCancel(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  function runNow() {
    if (firedRef.current) return;
    firedRef.current = true;
    onRun();
  }

  return (
    <div className="fixed inset-0 bg-deep/80 flex items-center justify-center z-50">
      <div role="dialog" aria-modal="true" aria-labelledby="eoq-title" className="bg-bg border border-border rounded-xl w-[420px] text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)]">
        <h3 id="eoq-title" className="font-bold px-4 py-3 border-b border-border">{t("eoq.title")}</h3>
        <p className="px-4 py-3 text-sm text-ink-soft">{t("eoq.body", { action: t(`end.${action}`), s })}</p>
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-border">
          <button type="button" ref={cancelRef} onClick={onCancel}
            className="px-3 py-1.5 rounded-md text-sm border border-line text-ink-soft hover:text-ink">{t("eoq.cancel")}</button>
          <button type="button" onClick={runNow}
            className="px-3 py-1.5 rounded-md text-sm border border-line bg-field text-ink hover:bg-surface">{t("eoq.now")}</button>
        </div>
      </div>
    </div>
  );
}
