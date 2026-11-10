import { useState } from "react";
import { t } from "../../i18n";

export type BatchMode = "direct" | "video" | "audio";

interface Props {
  busy: boolean;
  onSubmit(text: string, mode: BatchMode, hold: boolean): void;
}

export function BatchPanel({ busy, onSubmit }: Props) {
  const [text, setText] = useState("");
  const [mode, setMode] = useState<BatchMode>("direct");
  const disabled = busy || text.trim() === "";

  function submit(hold: boolean) {
    if (disabled) return;
    onSubmit(text, mode, hold);
  }

  return (
    <div className="flex flex-col gap-3 max-w-3xl">
      <textarea
        autoFocus
        aria-label={t("new.batch")}
        value={text}
        placeholder={t("new.batchPh")}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            submit(false);
          }
        }}
        className="w-full h-56 px-3 py-2 rounded-lg border border-border bg-field text-ink text-[13px] font-mono placeholder:text-ink-mute outline-none resize-none focus:border-accent focus:ring-1 focus:ring-accent/50"
      />
      <div className="flex items-center gap-3">
        <label className="flex items-center gap-2 text-[13px] text-ink-soft">
          {t("new.batchMode")}
          <select
            value={mode}
            onChange={(e) => setMode(e.target.value as BatchMode)}
            className="h-8 px-2 rounded-lg border border-border bg-field text-ink text-[13px] outline-none focus:border-accent"
          >
            <option value="direct">{t("new.batchDirect")}</option>
            <option value="video">{t("new.batchVideo")}</option>
            <option value="audio">{t("new.batchAudio")}</option>
          </select>
        </label>
        <div className="flex gap-2 ml-auto">
          <button type="button" disabled={disabled} onClick={() => submit(true)}
            className="h-9 px-4 rounded-lg border border-border bg-panel text-ink text-[13px] hover:bg-surface disabled:opacity-50 disabled:pointer-events-none">
            {t("new.addToQueue")}
          </button>
          <button type="button" disabled={disabled} onClick={() => submit(false)}
            className="h-9 px-4 rounded-lg bg-cta text-cta-ink text-[13px] font-semibold disabled:opacity-50 disabled:pointer-events-none">
            {t("new.download")}
          </button>
        </div>
      </div>
    </div>
  );
}
