import { useState } from "react";
import { MoreHorizontal, File, FolderOpen, Trash2, X, ChevronsUp, Minus, ChevronsDown } from "lucide-react";
import type { Priority } from "../../types";
import { t } from "../../i18n";

const PRIOS: { value: Priority; key: "prio.high" | "prio.normal" | "prio.low"; Icon: typeof Minus }[] = [
  { value: "high", key: "prio.high", Icon: ChevronsUp },
  { value: "normal", key: "prio.normal", Icon: Minus },
  { value: "low", key: "prio.low", Icon: ChevronsDown },
];

const ITEM = "block w-full text-left px-3 py-2 hover:bg-bg disabled:opacity-40";

export function RowMenu({
  canOpen,
  completed = false,
  priority,
  onOpenFile,
  onOpenFolder,
  onPriority,
  onCancel,
  onRemove,
}: {
  canOpen: boolean;
  completed?: boolean;
  priority: Priority;
  onOpenFile: () => void;
  onOpenFolder: () => void;
  onPriority: (p: Priority) => void;
  onCancel: () => void;
  onRemove: () => void;
}) {
  const [open, setOpen] = useState(false);
  const close = (fn: () => void) => () => { setOpen(false); fn(); };
  return (
    <div className="relative">
      <button
        className="inline-flex items-center justify-center w-7 h-7 rounded bg-field text-ink-soft hover:text-ink"
        onClick={() => setOpen((o) => !o)}
        title={t("dl.actions")}
        aria-label={t("dl.actions")}
      ><MoreHorizontal size={16} /></button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-8 z-20 w-52 bg-surface border border-border rounded-lg shadow-[0_20px_50px_rgba(0,0,0,.5)] text-sm text-ink overflow-hidden">
            <button className={ITEM} disabled={!canOpen} onClick={close(onOpenFile)}>
              <span className="inline-flex items-center gap-2"><File size={15} /> {t("dl.openFile")}</span>
            </button>
            <button className={ITEM} disabled={!canOpen} onClick={close(onOpenFolder)}>
              <span className="inline-flex items-center gap-2"><FolderOpen size={15} /> {t("dl.openFolder")}</span>
            </button>
            {!completed && (<div className="border-t border-border">
              {PRIOS.map(({ value, key, Icon }) => (
                <button
                  key={value}
                  className={`${ITEM} ${value === priority ? "text-accent-fg" : ""}`}
                  aria-current={value === priority ? "true" : undefined}
                  onClick={close(() => onPriority(value))}
                ><span className="inline-flex items-center gap-2"><Icon size={15} /> {t(key)}</span></button>
              ))}
            </div>)}
            <div className="border-t border-border">
              {!completed && (
                <button className={ITEM} onClick={close(onCancel)}>
                  <span className="inline-flex items-center gap-2"><X size={15} /> {t("dl.cancel")}</span>
                </button>
              )}
              <button className={ITEM} onClick={close(onRemove)}>
                <span className="inline-flex items-center gap-2"><Trash2 size={15} /> {t("dl.remove")}</span>
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
