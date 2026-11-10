import type { MediaFormat } from "../../api";
import { formatBytes } from "../../format";
import { t } from "../../i18n";

interface Props {
  formats: MediaFormat[];
  selected: number;
  onSelect(index: number): void;
}

export function FormatList({ formats, selected, onSelect }: Props) {
  return (
    <div role="radiogroup" className="flex flex-col gap-1.5">
      {formats.map((f) => {
        const on = f.index === selected;
        const detail = [f.container, f.codecs].filter(Boolean).join(" · ");
        return (
          <button
            key={f.index}
            type="button"
            role="radio"
            aria-checked={on}
            onClick={() => onSelect(f.index)}
            className={`grid grid-cols-[16px_64px_minmax(0,1fr)_auto_auto] items-center gap-3 px-3 py-2 rounded-lg border text-left text-[13px] bg-field outline-none focus-visible:ring-1 focus-visible:ring-accent/50 ${
              on ? "border-accent" : "border-border hover:border-line"
            }`}
          >
            <span className={`w-3.5 h-3.5 rounded-full border-2 ${on ? "border-accent bg-accent" : "border-ink-mute"}`} />
            <span className="font-mono font-bold text-ink">{f.label}</span>
            <span className="truncate text-ink-soft text-xs">
              {detail}
              {f.recommended && (
                <span className="ml-2 px-1.5 py-0.5 rounded bg-accent/15 text-accent text-[10px] font-semibold">{t("new.recommended")}</span>
              )}
            </span>
            <span className="font-mono text-xs text-ink-mute">{f.fps != null ? `${Math.round(f.fps)} i/s` : ""}</span>
            <span className="font-mono text-xs text-ink-soft w-16 text-right">{f.est_size != null ? formatBytes(f.est_size) : "—"}</span>
          </button>
        );
      })}
    </div>
  );
}
