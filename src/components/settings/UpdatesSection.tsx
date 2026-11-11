import { useEffect, type ReactNode } from "react";
import { Download, RefreshCw, Rocket } from "lucide-react";
import type { AppUpdateInfo, UpdateProgress } from "../../api";
import type { Settings } from "../../types";
import { t, type MsgKey } from "../../i18n";
import { Card, SectionShell, SettingRow, Switch, ghostBtn } from "./controls";
import { useVersions } from "./versions";
import { formatBytes, percent } from "../../format";
import { formatLastCheck, formatPublished } from "./updateFormat";
import { useAppUpdate, checkNow, downloadNow, installNow, refreshLastCheck } from "./appUpdateStore";

export type UpdateFlags = { app: AppUpdateInfo | null; ytdlp: string | null };

const primaryBtn =
  "h-8 px-3 rounded-lg bg-accent text-accent-ink text-[13px] font-bold inline-flex items-center gap-1.5 hover:opacity-90 outline-none focus-visible:ring-1 focus-visible:ring-accent/50 disabled:opacity-45";
const ghost = `${ghostBtn} inline-flex items-center gap-1.5 disabled:opacity-45`;

/** Ligne de carte dont le titre et l'indication peuvent contenir des versions en mono. */
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

/** Insère `node` à la place du marqueur `{name}` d'un texte traduit. */
function withSlot(key: MsgKey, name: string, node: ReactNode): ReactNode {
  const [before, after] = t(key, { [name]: "\u0000" }).split("\u0000");
  return <>{before}{node}{after}</>;
}

interface Props {
  settings: Settings;
  save(patch: Partial<Settings>): void;
  updates: UpdateFlags;
  onUpdatesChanged(patch: Partial<UpdateFlags>): void;
  recording: boolean;
  error?: string;
}

export function UpdatesSection({ settings, save, updates, onUpdatesChanged, recording, error }: Props) {
  const { app } = useVersions();
  const { state: phase, lastCheck } = useAppUpdate();
  useEffect(() => { void refreshLastCheck(); }, []);

  const info = updates.app;
  const mono = (v: string) => <span className="font-mono">{v}</span>;
  const check = () => void checkNow((found) => onUpdatesChanged({ app: found }));
  const retry = () => {
    if (phase.k !== "failed") return;
    if (phase.step === "check") check();
    else if (phase.step === "download") void downloadNow();
    else void installNow();
  };

  const searchBtn = (disabled: boolean) => (
    <button type="button" className={ghost} disabled={disabled} onClick={check}>
      <RefreshCw size={14} aria-hidden />{t("set.upd.search")}
    </button>
  );
  const notes = info?.notes?.trim();

  let row: ReactNode;
  if (phase.k === "checking") {
    row = <Row title={t("set.upd.checking")}>{searchBtn(true)}</Row>;
  } else if (phase.k === "failed") {
    row = (
      <Row
        title={t(phase.step === "check" ? "set.upd.checkFailed" : phase.step === "download" ? "set.upd.downloadFailed" : "set.upd.installFailed")}
        hint={phase.step === "check" ? t("set.upd.checkFailedHint") : phase.msg}
      >
        <span className="text-[11px] px-2 py-0.5 rounded-full font-semibold bg-err/10 text-err">{t("set.upd.pillFail")}</span>
        <button type="button" className={ghost} onClick={retry}><RefreshCw size={14} aria-hidden />{t("set.upd.retry")}</button>
      </Row>
    );
  } else if (info && phase.k === "downloading") {
    row = (
      <Row title={withSlot("set.upd.downloading", "version", mono(info.version))} hint={<DownloadProgress p={phase.p} />}>
        <button type="button" className={primaryBtn} disabled>{t("set.upd.install")}</button>
      </Row>
    );
  } else if (info && (phase.k === "ready" || phase.k === "installing")) {
    row = (
      <Row
        title={withSlot("set.upd.ready", "version", mono(info.version))}
        hint={`${t("set.upd.readyHint")}${recording ? ` ${t("set.upd.readyRecording")}` : ""}`}
      >
        <button type="button" className={primaryBtn} disabled={phase.k === "installing"} onClick={() => void installNow()}>
          <Rocket size={14} aria-hidden />{phase.k === "installing" ? t("set.upd.installing") : t("set.upd.restart")}
        </button>
      </Row>
    );
  } else if (info) {
    row = (
      <>
        <Row
          title={withSlot("set.upd.available", "version", mono(info.version))}
          hint={info.date ? withSlot("set.upd.published", "date", mono(formatPublished(info.date))) : undefined}
        >
          <span className="text-[11px] px-2 py-0.5 rounded-full font-semibold bg-accent/15 text-accent-fg">{t("set.upd.pillUpdate")}</span>
          <button type="button" className={primaryBtn} onClick={() => void downloadNow()}>
            <Download size={14} aria-hidden />{t("set.upd.install")}
          </button>
        </Row>
        {notes ? (
          <div className="px-4 pb-3.5 pt-1 text-[12.5px] text-ink-soft">
            <div className="font-semibold text-ink">{t("set.upd.notes")}</div>
            <p className="mt-1.5 whitespace-pre-line">{notes}</p>
          </div>
        ) : null}
      </>
    );
  } else if (phase.k === "uptodate") {
    row = (
      <Row title={t("set.upd.upToDate")}>
        <span className="text-[11px] px-2 py-0.5 rounded-full font-semibold bg-ok/10 text-ok">{t("set.upd.pillOk")}</span>
        {searchBtn(false)}
      </Row>
    );
  } else {
    row = <Row title={t("set.upd.idle")}>{searchBtn(false)}</Row>;
  }

  return (
    <SectionShell title={t("set.nav.updates")} desc={t("set.updates.desc")} error={error}>
      <Card title={t("set.upd.card")}>
        <Row
          title={t("set.upd.installed")}
          hint={lastCheck != null ? withSlot("set.upd.lastCheck", "when", mono(formatLastCheck(lastCheck, new Date()))) : undefined}
        >
          {app ? <span className="font-mono text-[13px] text-ink">{app}</span> : null}
        </Row>
        {row}
      </Card>

      <Card title={t("set.upd.options")}>
        <SettingRow label={t("set.upd.startupCheck")} hint={t("set.upd.startupCheckHint")}>
          <Switch checked={settings.app_update_check} label={t("set.upd.startupCheck")} onChange={(v) => save({ app_update_check: v })} />
        </SettingRow>
      </Card>
    </SectionShell>
  );
}

function DownloadProgress({ p }: { p: UpdateProgress | null }) {
  const pct = p ? percent(p.downloaded, p.total) : 0;
  return (
    <>
      <div
        role="progressbar"
        aria-label={t("set.upd.progressLabel")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={p?.total ? pct : undefined}
        className="h-1.5 rounded-[3px] bg-line overflow-hidden mt-2"
      >
        <div className="h-full bg-accent" style={{ width: `${pct}%` }} />
      </div>
      {p ? (
        <div className="mt-1 font-mono">
          {p.total
            ? t("set.upd.progress", { done: formatBytes(p.downloaded), total: formatBytes(p.total), pct })
            : t("set.upd.progressUnknown", { done: formatBytes(p.downloaded) })}
        </div>
      ) : null}
    </>
  );
}
