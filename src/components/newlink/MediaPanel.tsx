import { useState } from "react";
import { Play } from "lucide-react";
import type { FormatKind, LinkProbe } from "../../api";
import { formatBytes, hostOf } from "../../format";
import { t } from "../../i18n";
import {
  availableKinds, defaultFormat, formatDuration, formatsOf, outputExtension, validateCut, withExtension,
} from "../../newlink/selection";
import { DestinationFields } from "./DestinationFields";
import { FormatList } from "./FormatList";

export type MediaChoice = {
  formatIndex: number;
  subtitles: boolean;
  cut: { start: string; end: string } | null;
  dir: string;
  fileName: string;
};

interface Props {
  probe: Extract<LinkProbe, { type: "media" }>;
  busy: boolean;
  error?: string | null;
  onConfirm(choice: MediaChoice, startNow: boolean): void;
}

const TAB_LABEL = { av: "new.tabAv", audio: "new.tabAudio", video: "new.tabVideo" } as const;
const FIELD =
  "w-full h-8 px-2.5 rounded-lg border border-border bg-field text-ink text-[13px] font-mono placeholder:text-ink-mute outline-none focus:border-accent focus:ring-1 focus:ring-accent/50";

function Switch({ checked, disabled, label, onChange }: { checked: boolean; disabled?: boolean; label: string; onChange(v: boolean): void }) {
  return (
    <div className={`flex items-center justify-between gap-3 text-[13px] ${disabled ? "opacity-50" : ""}`}>
      <span className="text-ink">{label}</span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={`relative w-9 h-5 rounded-full transition-colors outline-none focus-visible:ring-1 focus-visible:ring-accent/50 ${checked ? "bg-accent" : "bg-line"}`}
      >
        <span className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform ${checked ? "translate-x-4" : ""}`} />
      </button>
    </div>
  );
}

export function MediaPanel({ probe, busy, error, onConfirm }: Props) {
  const { info, formats } = probe;
  const kinds = availableKinds(formats);
  const [kind, setKind] = useState<FormatKind>(kinds[0] ?? "av");
  const [formatIndex, setFormatIndex] = useState<number>(defaultFormat(formats, kinds[0] ?? "av")?.index ?? 0);
  const [subtitles, setSubtitles] = useState(false);
  const [cutOn, setCutOn] = useState(false);
  const [start, setStart] = useState("");
  const [end, setEnd] = useState("");
  const [dirEdit, setDirEdit] = useState<string | null>(null);
  const [nameEdit, setNameEdit] = useState<string | null>(null);

  const format = formats.find((f) => f.index === formatIndex);
  const dir = dirEdit ?? (kind === "audio" ? probe.music_dir : probe.video_dir);
  const ext = format ? outputExtension(format) : ".mp4";
  // Le radical saisi est gardé, l'extension suit toujours le format (envoyé à Rust ainsi).
  const stem = nameEdit ?? probe.file_name;
  const fileName = stem.trim() === "" ? "" : withExtension(stem, ext);
  // Affiché : la saisie brute pendant la frappe (pas de saut de curseur) ; réécrite à
  // chaque changement de format par `selectFormat`.
  const shownName = nameEdit ?? fileName;
  const cutError = cutOn ? validateCut(start, end, info.duration_s) : null;
  const subsOn = subtitles && kind !== "audio";
  const subsAvailable = info.subtitle_langs.length > 0;
  const disabled = busy || cutError !== null || fileName.trim() === "";
  const size = format?.est_size != null ? formatBytes(format.est_size) : null;

  function selectFormat(index: number) {
    setFormatIndex(index);
    const f = formats.find((x) => x.index === index);
    if (f) setNameEdit((n) => (n === null || n.trim() === "" ? n : withExtension(n, outputExtension(f))));
  }

  function pickKind(k: FormatKind) {
    setKind(k);
    const f = defaultFormat(formats, k);
    if (f) selectFormat(f.index);
  }

  function confirm(startNow: boolean) {
    onConfirm({
      formatIndex,
      subtitles: subsOn,
      cut: cutOn ? { start, end } : null,
      dir,
      fileName,
    }, startNow);
  }

  const meta = [info.uploader, hostOf(probe.url)].filter(Boolean).join(" · ");

  return (
    <div className="grid grid-cols-[320px_minmax(0,1fr)] gap-6">
      <div className="flex flex-col gap-3 min-w-0">
        <div className="relative aspect-video rounded-lg overflow-hidden bg-gradient-to-br from-surface to-panel flex items-center justify-center text-ink-mute">
          {info.thumbnail ? (
            <img src={info.thumbnail} alt="" className="w-full h-full object-cover" />
          ) : (
            <Play size={32} />
          )}
          {info.duration_s != null && (
            <span className="absolute bottom-1.5 right-1.5 px-1.5 text-[10px] font-mono rounded bg-black/70 text-white">
              {formatDuration(info.duration_s)}
            </span>
          )}
        </div>
        <div className="min-w-0">
          <div className="text-sm font-semibold text-ink leading-snug">{info.title}</div>
          <div className="mt-1 text-xs text-ink-soft truncate">
            {meta}
            {info.upload_date ? ` · ${info.upload_date}` : ""}
          </div>
        </div>
        <div className="flex flex-wrap gap-1.5">
          <span className="px-2 py-0.5 rounded-full bg-surface text-ink-soft text-[11px]">{t("new.formats", { n: formats.length })}</span>
          {subsAvailable && (
            <span className="px-2 py-0.5 rounded-full bg-surface text-ink-soft text-[11px]">
              {t("new.subsAvailable", { langs: info.subtitle_langs.join(", ") })}
            </span>
          )}
        </div>
        <div className="flex flex-col gap-2.5 p-3 rounded-lg bg-panel border border-line">
          <div className="text-[11px] uppercase tracking-wide text-ink-mute">{t("new.options")}</div>
          <Switch label={t("new.optSubs")} checked={subsOn} disabled={!subsAvailable || kind === "audio"} onChange={setSubtitles} />
          {info.can_cut && <Switch label={t("new.optCut")} checked={cutOn} onChange={(v) => { setCutOn(v); if (v && start === "") setStart("0:00"); }} />}
          {info.can_cut && cutOn && (
            <div className="flex flex-col gap-1.5">
              <div className="grid grid-cols-2 gap-2">
                <label className="text-xs text-ink-soft">
                  {t("new.cutStart")}
                  <input value={start} onChange={(e) => setStart(e.target.value)} placeholder="0:00" className={`${FIELD} mt-1`} />
                </label>
                <label className="text-xs text-ink-soft">
                  {t("new.cutEnd")}
                  <input value={end} onChange={(e) => setEnd(e.target.value)} placeholder={t("new.cutEndPh")} className={`${FIELD} mt-1`} />
                </label>
              </div>
              {cutError && <div className="text-xs text-err">{t(cutError)}</div>}
            </div>
          )}
        </div>
      </div>

      <div className="flex flex-col gap-4 min-w-0">
        <div className="inline-flex self-start p-0.5 rounded-lg bg-surface">
          {kinds.map((k) => (
            <button
              key={k}
              type="button"
              onClick={() => pickKind(k)}
              className={`px-3 h-8 rounded-md text-[13px] ${k === kind ? "bg-panel shadow-sm text-ink font-medium" : "text-ink-soft hover:text-ink"}`}
            >
              {t(TAB_LABEL[k])}
            </button>
          ))}
        </div>
        <FormatList formats={formatsOf(formats, kind)} selected={formatIndex} onSelect={selectFormat} />
        <DestinationFields dir={dir} fileName={shownName} onDir={setDirEdit} onFileName={setNameEdit} />
        {error && <div role="alert" className="text-xs text-err">{error}</div>}
        <div className="flex justify-end gap-2">
          <button type="button" disabled={disabled} onClick={() => confirm(false)}
            className="h-9 px-4 rounded-lg border border-border bg-panel text-ink text-[13px] hover:bg-surface disabled:opacity-50 disabled:pointer-events-none">
            {t("new.addToQueue")}
          </button>
          <button type="button" disabled={disabled} onClick={() => confirm(true)}
            className="h-9 px-4 rounded-lg bg-cta text-cta-ink text-[13px] font-semibold disabled:opacity-50 disabled:pointer-events-none">
            {size ? t("new.downloadSize", { size }) : t("new.download")}
          </button>
        </div>
      </div>
    </div>
  );
}
