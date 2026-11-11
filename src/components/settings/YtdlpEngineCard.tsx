import { useEffect, type ReactNode } from "react";
import { RefreshCw } from "lucide-react";
import * as api from "../../api";
import type { UpdateProgress } from "../../api";
import type { Settings } from "../../types";
import { t, type MsgKey } from "../../i18n";
import { Card, SettingRow, Switch, ghostBtn } from "./controls";
import { formatBytes, percent } from "../../format";
import { formatLastCheck } from "./updateFormat";
import {
  useYtdlpUpdate, checkYtdlpNow, updateYtdlpNow, applyUpdated, refreshVersion, refreshLastCheck,
} from "./ytdlpUpdateStore";

const primaryBtn =
  "h-8 px-3 rounded-lg bg-accent text-accent-ink text-[13px] font-bold inline-flex items-center gap-1.5 hover:opacity-90 outline-none focus-visible:ring-1 focus-visible:ring-accent/50 disabled:opacity-45";
const ghost = `${ghostBtn} inline-flex items-center gap-1.5 disabled:opacity-45`;

const mono = (v: string) => <span className="font-mono">{v}</span>;

/** Insère `node` à la place du marqueur `{name}` d'un texte traduit. */
function withSlot(key: MsgKey, name: string, node: ReactNode): ReactNode {
  const [before, after] = t(key, { [name]: "\u0000" }).split("\u0000");
  return <>{before}{node}{after}</>;
}

function Row({ title, hint, children }: { title: ReactNode; hint?: ReactNode; children?: ReactNode }) {
  return (
    <div className="flex items-center gap-4 px-4 py-3 border-b border-line last:border-b-0">
      <div className="flex-1 min-w-0">
        <div className="text-[13px] font-semibold text-ink">{title}</div>
        {hint ? <div className="text-[12px] text-ink-mute">{hint}</div> : null}
      </div>
      <div className="flex items-center gap-2 shrink-0">{children}</div>
    </div>
  );
}

function Pill({ tone, children }: { tone: "ok" | "up" | "err"; children: ReactNode }) {
  const cls = tone === "ok" ? "bg-ok/10 text-ok" : tone === "err" ? "bg-err/10 text-err" : "bg-accent/15 text-accent-fg";
  return <span className={`text-[11px] px-2 py-0.5 rounded-full font-semibold ${cls}`}>{children}</span>;
}

interface Props {
  settings: Settings;
  save(patch: Partial<Settings>): void;
  /** Dernière version disponible connue (null si aucune). */
  latest: string | null;
  /** Appelé avec la version disponible, ou null (recherche sans résultat, mise à jour terminée). */
  onUpdated(latest: string | null): void;
}

export function YtdlpEngineCard({ settings, save, latest, onUpdated }: Props) {
  const { state, version, lastCheck, pending } = useYtdlpUpdate();

  useEffect(() => {
    void refreshVersion();
    void refreshLastCheck();
    let alive = true;
    let un: (() => void) | undefined;
    // Mise à jour automatique au démarrage : relire la version et l'éventuelle attente.
    Promise.resolve(api.onYtdlpUpdated((p) => { void applyUpdated(p, onUpdated); }))
      .then((f) => { if (alive) un = f; else f?.(); })
      .catch(() => {});
    return () => { alive = false; un?.(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const check = () => void checkYtdlpNow(onUpdated);
  const update = () => { if (latest) void updateYtdlpNow(latest, onUpdated); };
  const title = (
    <>yt-dlp{version ? <> <span className="font-mono font-medium text-ink-mute">{version}</span></> : null}</>
  );

  const searchBtn = (disabled = false) => (
    <button type="button" className={ghost} disabled={disabled} onClick={check}>
      <RefreshCw size={14} aria-hidden />{t("set.yt.search")}
    </button>
  );
  const updateBtn = (disabled = false) => (
    <button type="button" className={primaryBtn} disabled={disabled} onClick={update}>
      <RefreshCw size={14} aria-hidden />{t("set.yt.update")}
    </button>
  );

  const lastCheckHint = lastCheck != null
    ? withSlot("set.yt.lastCheck", "when", mono(formatLastCheck(lastCheck, new Date())))
    : undefined;

  let row: ReactNode;
  if (state.k === "updating") {
    row = (
      <Row title={withSlot("set.yt.updating", "version", mono(state.target))} hint={<Progress p={state.p} />}>
        {updateBtn(true)}
      </Row>
    );
  } else if (state.k === "checking") {
    row = <Row title={title} hint={t("set.yt.checking")}>{searchBtn(true)}</Row>;
  } else if (state.k === "failed") {
    const retry = state.step === "update" && latest ? update : check;
    row = (
      <Row title={title} hint={<>{state.msg}{state.step === "update" ? ` ${t("set.yt.keeps")}` : ""}</>}>
        <Pill tone="err">{t("set.yt.pillFail")}</Pill>
        <button type="button" className={ghost} onClick={retry}><RefreshCw size={14} aria-hidden />{t("set.yt.retry")}</button>
      </Row>
    );
  } else if (latest && latest !== pending) {
    row = (
      <Row title={title} hint={withSlot("set.yt.available", "version", mono(latest))}>
        <Pill tone="up">{t("set.yt.pillUpdate")}</Pill>
        {updateBtn()}
      </Row>
    );
  } else if (pending) {
    row = (
      <Row title={title} hint={withSlot("set.yt.pending", "version", mono(pending))}>
        {searchBtn()}
      </Row>
    );
  } else {
    row = (
      <Row title={title} hint={lastCheckHint}>
        {state.k === "uptodate" ? <Pill tone="ok">{t("set.yt.pillOk")}</Pill> : null}
        {searchBtn()}
      </Row>
    );
  }

  return (
    <Card title={t("set.engineCard")}>
      {version === null && state.k === "idle" && !latest ? (
        <Row title="yt-dlp" hint={t("set.ytdlpUnknown")}>{searchBtn()}</Row>
      ) : row}
      <SettingRow label={t("set.yt.auto")} hint={t("set.yt.autoHint")}>
        <Switch checked={settings.ytdlp_auto_update} label={t("set.yt.auto")} onChange={(v) => save({ ytdlp_auto_update: v })} />
      </SettingRow>
    </Card>
  );
}

function Progress({ p }: { p: UpdateProgress | null }) {
  const pct = p ? percent(p.downloaded, p.total) : 0;
  return (
    <>
      <div
        role="progressbar"
        aria-label={t("set.yt.progressLabel")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={p?.total ? pct : undefined}
        className="h-1.5 rounded-[3px] bg-line overflow-hidden mt-2"
      >
        <div className="h-full bg-accent" style={{ width: `${pct}%` }} />
      </div>
      {p ? (
        <div data-testid="ytdlp-progress" className="mt-1 font-mono">
          {p.total
            ? t("set.yt.progress", { done: formatBytes(p.downloaded), total: formatBytes(p.total) })
            : t("set.yt.progressUnknown", { done: formatBytes(p.downloaded) })}
        </div>
      ) : null}
    </>
  );
}
