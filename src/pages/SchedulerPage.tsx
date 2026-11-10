import { useEffect, useState } from "react";
import * as api from "../api";
import type { EndAction, Schedule } from "../api";
import { t } from "../i18n";
import type { MsgKey } from "../i18n";
import { nextStartLabel, timelineSegments } from "../queue/schedule";

const MB = 1048576;
const DAYS = [0, 1, 2, 3, 4, 5, 6];
const TICKS = [0, 6, 12, 18, 24];
const ACTIONS: EndAction[] = ["none", "quit", "sleep", "hibernate", "shutdown"];
const CARD = "rounded-xl border border-border bg-panel p-4 flex flex-col gap-3";
const FIELD =
  "h-8 px-2.5 rounded-lg border border-border bg-field text-ink text-[13px] font-mono outline-none focus:border-accent focus:ring-1 focus:ring-accent/50 disabled:opacity-50";

function Switch({ checked, label, onChange }: { checked: boolean; label: string; onChange(v: boolean): void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative w-9 h-5 shrink-0 rounded-full transition-colors outline-none focus-visible:ring-1 focus-visible:ring-accent/50 ${checked ? "bg-accent" : "bg-line"}`}
    >
      <span className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform ${checked ? "translate-x-4" : ""}`} />
    </button>
  );
}

function Err({ msg }: { msg?: string }) {
  return msg ? <p role="alert" className="text-[12px] text-err">{msg}</p> : null;
}

export function SchedulerPage({ heldCount }: { heldCount: number }) {
  const [sch, setSch] = useState<Schedule | null>(null);
  const [endAction, setEnd] = useState<EndAction>("none");
  const [limitText, setLimitText] = useState("");
  const [errWhen, setErrWhen] = useState<string>();
  const [errDuring, setErrDuring] = useState<string>();
  const [errDone, setErrDone] = useState<string>();

  useEffect(() => {
    let alive = true;
    api.getSchedule().then((s) => {
      if (!alive) return;
      setSch(s);
      setLimitText(s.limit_bps != null ? String(s.limit_bps / MB) : "");
    }).catch(() => {});
    api.getEndAction().then((a) => { if (alive) setEnd(a); }).catch(() => {});
    return () => { alive = false; };
  }, []);

  if (!sch) return null;

  const save = (next: Schedule, setErr: (m?: string) => void) => {
    setSch(next);
    setErr(undefined);
    api.setSchedule(next).catch((e) => setErr(String(e)));
  };

  const label = sch.enabled ? nextStartLabel(sch, new Date()) : null;
  const limitOn = sch.limit_bps != null;
  const segs = timelineSegments(sch);

  const onLimitText = (v: string) => {
    setLimitText(v);
    const n = Number(v.replace(",", "."));
    if (v.trim() !== "" && Number.isFinite(n) && n > 0) save({ ...sch, limit_bps: Math.round(n * MB) }, setErrDuring);
  };
  const toggleLimit = (on: boolean) => {
    if (!on) return save({ ...sch, limit_bps: null }, setErrDuring);
    const n = Number(limitText.replace(",", "."));
    const mb = Number.isFinite(n) && n > 0 ? n : 5;
    setLimitText(String(mb));
    save({ ...sch, limit_bps: Math.round(mb * MB) }, setErrDuring);
  };

  return (
    <div className="flex flex-col gap-4 p-5 max-w-[720px]">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-[16px] font-semibold text-ink">{t("sch.title")}</h1>
          <p className="text-[12.5px] text-ink-dim">
            {sch.enabled && label ? t("sch.next", { when: label, n: heldCount }) : t("sch.off")}
          </p>
        </div>
        <div className="flex items-center gap-2 text-[13px] text-ink">
          <span>{t("sch.enabled")}</span>
          <Switch checked={sch.enabled} label={t("sch.enabled")} onChange={(v) => save({ ...sch, enabled: v }, setErrWhen)} />
        </div>
      </div>

      <section className={CARD}>
        <h2 className="text-[13px] font-semibold text-ink">{t("sch.when")}</h2>
        <div className="flex items-center gap-2 text-[13px] text-ink-dim">
          <span>{t("sch.startAt")}</span>
          <input type="time" aria-label={t("sch.startAt")} className={FIELD} value={sch.start}
            onChange={(e) => e.target.value && save({ ...sch, start: e.target.value }, setErrWhen)} />
          <span>{t("sch.until")}</span>
          <input type="time" aria-label={t("sch.until")} className={FIELD} value={sch.end}
            onChange={(e) => e.target.value && save({ ...sch, end: e.target.value }, setErrWhen)} />
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[12.5px] text-ink-dim mr-1">{t("sch.days")}</span>
          {DAYS.map((d) => (
            <button key={d} type="button" aria-label={t(`day.${d}` as MsgKey)} aria-pressed={sch.days[d]}
              onClick={() => save({ ...sch, days: sch.days.map((v, i) => (i === d ? !v : v)) }, setErrWhen)}
              className={`w-8 h-8 rounded-full text-[12px] font-medium border transition-colors ${sch.days[d] ? "bg-accent text-accent-fg border-accent" : "bg-field text-ink-dim border-border"}`}>
              {t(`dayShort.${d}` as MsgKey)}
            </button>
          ))}
        </div>
        <div>
          <div data-testid="timeline" className="relative h-3 rounded-full bg-surface border border-line overflow-hidden">
            {segs.map((s, i) => (
              <div key={i} data-seg className="absolute top-0 bottom-0 bg-accent/70"
                style={{ left: `${s.left}%`, width: `${s.width}%` }} />
            ))}
          </div>
          <div className="relative h-4 mt-1 text-[11px] font-mono text-ink-mute">
            {TICKS.map((h) => (
              <span key={h} className="absolute -translate-x-1/2 first:translate-x-0 last:-translate-x-full"
                style={{ left: `${(h / 24) * 100}%` }}>
                {String(h).padStart(2, "0")}h
              </span>
            ))}
          </div>
        </div>
        <Err msg={errWhen} />
      </section>

      <section className={CARD}>
        <h2 className="text-[13px] font-semibold text-ink">{t("sch.during")}</h2>
        <div className="flex items-center gap-3 text-[13px] text-ink">
          <Switch checked={limitOn} label={t("sch.limit")} onChange={toggleLimit} />
          <span>{t("sch.limit")}</span>
          <input type="number" min="0.1" step="any" aria-label="Mo/s" className={`${FIELD} w-24`} disabled={!limitOn}
            value={limitText} onChange={(e) => onLimitText(e.target.value)} />
          <span className="text-ink-dim">Mo/s</span>
        </div>
        <p className="text-[12px] text-ink-mute">{t("sch.limitHint")}</p>
        <label className="flex items-center gap-2 text-[13px] text-ink">
          <span>{t("sch.atEnd", { end: sch.end })}</span>
          <select className={FIELD} value={sch.at_end}
            onChange={(e) => save({ ...sch, at_end: e.target.value as Schedule["at_end"] }, setErrDuring)}>
            <option value="pause">{t("sch.atEndPause")}</option>
            <option value="continue">{t("sch.atEndContinue")}</option>
          </select>
        </label>
        <Err msg={errDuring} />
      </section>

      <section className={CARD}>
        <h2 className="text-[13px] font-semibold text-ink">{t("sch.done")}</h2>
        <label className="flex items-center gap-2 text-[13px] text-ink">
          <span>{t("sch.action")}</span>
          <select aria-label={t("sch.action")} className={FIELD} value={endAction}
            onChange={(e) => {
              const a = e.target.value as EndAction;
              setEnd(a);
              setErrDone(undefined);
              api.setEndAction(a).catch((er) => setErrDone(String(er)));
            }}>
            {ACTIONS.map((a) => <option key={a} value={a}>{t(`end.${a}` as MsgKey)}</option>)}
          </select>
        </label>
        <p className="text-[12px] text-ink-mute">{t("sch.countdownNote")}</p>
        <Err msg={errDone} />
      </section>
    </div>
  );
}
