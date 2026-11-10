import { useId } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";
import { t } from "../../i18n";

interface Props {
  dir: string;
  fileName: string;
  onDir(d: string): void;
  onFileName(n: string): void;
}

const FIELD =
  "w-full h-9 px-3 rounded-lg border border-border bg-field text-ink text-[13px] outline-none focus:border-accent focus:ring-1 focus:ring-accent/50";

export function DestinationFields({ dir, fileName, onDir, onFileName }: Props) {
  const dirId = useId();
  const nameId = useId();

  async function browse() {
    const picked = await open({ directory: true, defaultPath: dir });
    if (typeof picked === "string" && picked) onDir(picked);
  }

  return (
    <div className="flex flex-col gap-3">
      <div>
        <label htmlFor={dirId} className="block mb-1 text-xs text-ink-soft">{t("new.saveIn")}</label>
        <div className="flex gap-2">
          <input id={dirId} value={dir} onChange={(e) => onDir(e.target.value)} className={`${FIELD} font-mono`} />
          <button
            type="button"
            onClick={() => void browse()}
            className="shrink-0 h-9 px-3 inline-flex items-center gap-1.5 rounded-lg border border-border bg-panel text-ink text-[13px] hover:bg-surface"
          >
            <FolderOpen size={14} />
            {t("new.browse")}
          </button>
        </div>
      </div>
      <div>
        <label htmlFor={nameId} className="block mb-1 text-xs text-ink-soft">{t("new.fileName")}</label>
        <input id={nameId} value={fileName} onChange={(e) => onFileName(e.target.value)} className={FIELD} />
      </div>
    </div>
  );
}
