import { useEffect, useRef, useState } from "react";
import { ClipboardPaste, Layers, Link, Loader2, RotateCcw } from "lucide-react";
import * as api from "../api";
import type { ConfirmLink, LinkProbe } from "../api";
import { t, tNodes } from "../i18n";
import { useLinkProbe } from "../newlink/useLinkProbe";
import { BatchPanel, type BatchMode } from "../components/newlink/BatchPanel";
import { FilePanel } from "../components/newlink/FilePanel";
import { MediaPanel, type MediaChoice } from "../components/newlink/MediaPanel";

export type ExternalEvent = { seq: number } & (
  | { kind: "probing"; url: string }
  | { kind: "probed"; probe: LinkProbe }
  | { kind: "error"; url: string; message: string; captureId?: string; origin?: string }
  | { kind: "analyze"; url: string }
);

const secondaryBtn =
  "inline-flex items-center gap-1.5 h-9 px-3 rounded-lg border border-border bg-field text-ink text-[13px] font-medium hover:bg-surface";

const errText = (e: unknown) => (typeof e === "string" ? e : String((e as { message?: unknown })?.message ?? e));
const isUrl = (s: string) => /^https?:\/\//i.test(s.trim());

/** Explication sous l'erreur d'une capture ; l'origine (domaine) est en mono. */
function captureNote(origin?: string) {
  if (!origin) return t("new.captureNoteNoOrigin");
  return tNodes("new.captureNote", { origin: <span className="font-mono">{origin}</span> });
}

export function NewPage({
  external, startInBatch, onDone,
}: {
  external: ExternalEvent | null;
  startInBatch: boolean;
  onDone(info?: string): void;
}) {
  const { state, analyze, showProbing, load, fail, reset } = useLinkProbe();
  const [url, setUrl] = useState("");
  const [batch, setBatch] = useState(startInBatch);
  const [busy, setBusy] = useState(false);
  const [confirmError, setConfirmError] = useState<string | null>(null);
  const [batchError, setBatchError] = useState<string | null>(null);
  const lastSeq = useRef<number | null>(null);
  const retrySeq = useRef(0);
  // Capture de l'extension en échec : « Réessayer » la renvoie avec son contexte (cookies, page d'origine).
  const [capture, setCapture] = useState<{ id: string; origin?: string; url: string } | null>(null);

  useEffect(() => { setBatch(startInBatch); }, [startInBatch]);

  useEffect(() => {
    if (!external || external.seq === lastSeq.current) return;
    lastSeq.current = external.seq;
    retrySeq.current += 1;
    setBatch(false);
    setConfirmError(null);
    setCapture(null);
    if (external.kind === "probing") {
      setUrl(external.url);
      showProbing(external.url);
    } else if (external.kind === "probed") {
      setUrl(external.probe.url);
      load(external.probe);
    } else if (external.kind === "analyze") {
      setUrl(external.url);
      analyze(external.url);
    } else {
      setUrl(external.url);
      if (external.captureId) setCapture({ id: external.captureId, origin: external.origin, url: external.url });
      fail(external.url, external.message);
    }
  }, [external, showProbing, load, fail, analyze]);

  function start(value: string) {
    retrySeq.current += 1;
    setConfirmError(null);
    setCapture(null);
    analyze(value);
  }

  // Les résultats arrivent par les événements link:// ; showProbing garde le délai de 90 s.
  function retryFromCapture(c: { id: string; url: string }) {
    const seq = ++retrySeq.current;
    showProbing(c.url);
    api.retryCapture(c.id, c.url).catch((e) => { if (seq === retrySeq.current) fail(c.url, errText(e)); });
  }

  async function pasteFromClipboard() {
    try {
      const text = (await navigator.clipboard.readText()).trim();
      if (!text) return;
      setUrl(text);
      if (isUrl(text)) start(text);
    } catch {
      /* presse-papiers indisponible : ignoré */
    }
  }

  async function confirm(req: Omit<ConfirmLink, "probe_id" | "start_now">, startNow: boolean) {
    if (state.status !== "ready") return;
    setBusy(true);
    setConfirmError(null);
    try {
      await api.confirmLink({ probe_id: state.probe.probe_id, ...req, start_now: startNow });
      reset();
      setUrl("");
      onDone();
    } catch (e) {
      setConfirmError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  function confirmMedia(c: MediaChoice, startNow: boolean) {
    return confirm({
      format_index: c.formatIndex,
      subtitles: c.subtitles,
      cut: c.cut ? { start: c.cut.start, end: c.cut.end.trim() === "" ? null : c.cut.end } : null,
      dest_dir: c.dir,
      file_name: c.fileName,
    }, startNow);
  }

  function confirmFile(c: { dir: string; fileName: string }, startNow: boolean) {
    return confirm({ format_index: null, subtitles: false, cut: null, dest_dir: c.dir, file_name: c.fileName }, startNow);
  }

  async function submitBatch(text: string, mode: BatchMode, hold: boolean) {
    setBusy(true);
    setBatchError(null);
    try {
      const s = await api.addBatchDownloads(text, "", mode, hold);
      const parts = [t("batch.added", { n: s.added })];
      if (s.skipped_duplicate) parts.push(t("batch.duplicates", { n: s.skipped_duplicate }));
      if (s.skipped_invalid) parts.push(t("batch.invalid", { n: s.skipped_invalid }));
      if (s.failed) parts.push(t("batch.failed", { n: s.failed }));
      if (s.not_replaced) parts.push(t("batch.notReplaced", { n: s.not_replaced }));
      onDone(parts.join(" · "));
    } catch (e) {
      setBatchError(`Échec de l'ajout en lot : ${errText(e)}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col h-full p-5 gap-4 overflow-y-auto">
      <h1 className="text-[20px] font-extrabold text-ink">{t("new.title")}</h1>
      <div className="flex items-center gap-3">
        <div className="relative flex-1">
          <Link size={15} className="absolute left-3 top-1/2 -translate-y-1/2 text-ink-mute" />
          <input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && url.trim()) start(url); }}
            onPaste={(e) => {
              const text = e.clipboardData.getData("text").trim();
              if (isUrl(text)) {
                e.preventDefault();
                setUrl(text);
                setBatch(false);
                start(text);
              }
            }}
            placeholder={t("new.urlPh")}
            className="w-full h-10 pl-9 pr-24 rounded-lg border border-border bg-field text-ink text-[13px] placeholder:text-ink-mute outline-none focus:border-accent focus:ring-1 focus:ring-accent/50"
          />
          <button
            type="button"
            onClick={pasteFromClipboard}
            className="absolute right-1.5 top-1/2 -translate-y-1/2 inline-flex items-center gap-1 h-7 px-2 rounded-md text-xs text-ink-soft hover:bg-surface hover:text-ink"
          >
            <ClipboardPaste size={14} />
            {t("new.paste")}
          </button>
        </div>
        <button type="button" className={secondaryBtn} onClick={() => setBatch((b) => !b)}>
          <Layers size={15} />
          {batch ? t("new.single") : t("new.batch")}
        </button>
      </div>

      {batch ? (
        <>
          {batchError && <div role="alert" className="text-xs text-err">{batchError}</div>}
          <BatchPanel busy={busy} onSubmit={submitBatch} />
        </>
      ) : (
        <>
          {state.status === "idle" && <p className="text-[13px] text-ink-soft">{t("new.hint")}</p>}
          {state.status === "probing" && (
            <div className="flex items-center gap-3 text-[13px] text-ink-soft">
              <Loader2 size={16} className="animate-spin text-accent" />
              <span>{t("new.probing")}</span>
              <button type="button" className={secondaryBtn} onClick={reset}>{t("new.cancel")}</button>
            </div>
          )}
          {state.status === "error" && (
            <div className="flex items-center gap-3 text-[13px]">
              <div className="flex flex-col gap-1">
                <span role="alert" className="text-err">{state.message}</span>
                {capture && capture.url === state.url && (
                  <span className="text-[12px] text-ink-mute">{captureNote(capture.origin)}</span>
                )}
              </div>
              <button
                type="button"
                className={secondaryBtn}
                onClick={() => (capture && capture.url === state.url ? retryFromCapture(capture) : start(state.url))}
              >
                <RotateCcw size={14} />
                {t("new.retry")}
              </button>
            </div>
          )}
          {state.status === "ready" && state.probe.type === "media" && (
            <MediaPanel key={state.probe.probe_id} probe={state.probe} busy={busy} error={confirmError} onConfirm={confirmMedia} />
          )}
          {state.status === "ready" && state.probe.type === "file" && (
            <FilePanel key={state.probe.probe_id} probe={state.probe} busy={busy} error={confirmError} onConfirm={confirmFile} />
          )}
        </>
      )}
    </div>
  );
}
