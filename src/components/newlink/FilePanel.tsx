import { useState } from "react";
import type { LinkProbe } from "../../api";
import { formatBytes } from "../../format";
import { t } from "../../i18n";
import { CategoryIcon } from "../categoryIcons";
import { DestinationFields } from "./DestinationFields";

interface Props {
  probe: Extract<LinkProbe, { type: "file" }>;
  busy: boolean;
  error?: string | null;
  onConfirm(choice: { dir: string; fileName: string }, startNow: boolean): void;
}

export function FilePanel({ probe, busy, error, onConfirm }: Props) {
  const [dir, setDir] = useState(probe.default_dir);
  const [fileName, setFileName] = useState(probe.file_name);
  const disabled = busy || fileName.trim() === "";
  const mark = (ok: boolean) => (ok ? "✓" : "✗");
  const size = probe.total_bytes != null ? formatBytes(probe.total_bytes) : null;
  const info = [
    size,
    t(`cat.${probe.category}` as const),
    `${t("new.resumable")} ${mark(probe.supports_ranges)}`,
    `${t("new.accelerated")} ${mark(probe.supports_ranges)}`,
  ].filter(Boolean).join(" · ");

  return (
    <div className="flex flex-col gap-4 max-w-xl">
      <div className="flex items-center gap-3 p-3 rounded-lg bg-panel border border-line">
        <div className="w-11 h-11 rounded-lg bg-surface flex items-center justify-center text-accent">
          <CategoryIcon cat={probe.category} size={22} />
        </div>
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-ink">{probe.file_name}</div>
          <div className="text-xs text-ink-soft font-mono">{info}</div>
        </div>
      </div>
      <DestinationFields dir={dir} fileName={fileName} onDir={setDir} onFileName={setFileName} />
      {error && <div role="alert" className="text-xs text-err">{error}</div>}
      <div className="flex justify-end gap-2">
        <button type="button" disabled={disabled} onClick={() => onConfirm({ dir, fileName }, false)}
          className="h-9 px-4 rounded-lg border border-border bg-panel text-ink text-[13px] hover:bg-surface disabled:opacity-50 disabled:pointer-events-none">
          {t("new.addToQueue")}
        </button>
        <button type="button" disabled={disabled} onClick={() => onConfirm({ dir, fileName }, true)}
          className="h-9 px-4 rounded-lg bg-cta text-cta-ink text-[13px] font-semibold disabled:opacity-50 disabled:pointer-events-none">
          {size ? t("new.downloadSize", { size }) : t("new.download")}
        </button>
      </div>
    </div>
  );
}
