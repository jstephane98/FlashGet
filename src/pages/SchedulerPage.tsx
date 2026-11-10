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
      <span className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full ${checked ? "bg-accent-fg translate-x-4" : "bg-panel"} transition-transform`} />
    </button>
  );
}

export function SchedulerPage({ heldCount }: { heldCount: number }) {
  const [sch, setSch] = useState<Schedule | null>(null);
  const [endAction, setEnd] = useState<EndAction | null>(null);
  const [limitText, setLimitText] = useState("");
  const [loadFailed, setLoadFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let alive = true;
    setLoadFailed(false);
    api.getSchedule().then((s) => {
      if (!alive) return;
      setSch(s);
      setLimitText(s.limit_bps != null ? String(s.limit_bps / MB) : "");
    }).catch(() => { if (alive) setLoadFailed(true); });
    api.getEndAction().then((a) => { if (alive) setEnd(a); }).catch(() => { if (alive) setLoadFailed(true); });
    return () => { alive = false; };
  }, [attempt]);

  if (!sch) {
    return loadFailed ? (
      <div className="flex flex-col items-start gap-3 p-5">
        <p role="alert" className="text-[13px] text-err">{t("sch.loadError")}</p>
        <button type="button" onClick={() => setAttempt((n) => n + 1)}
          className="h-8 px-3 rounded-lg border border-border bg-field text-ink text-[13px]">
          {t("new.retry")}
        </button>
      </div>
    ) : null;
  }

  // Un seul message d'erreur pour la page : une plage invalide peut faire échouer l'enregistrement d'une autre carte.
  const save = (next: Schedule) => {
    setSch(next);
    setError(undefined);
    api.setSchedule(next).catch((e) => setError(String(e)));
  };

  const label = sch.enabled ? nextStartLabel(sch, new Date()) : null;
  const limitOn = sch.limit_bps != null;
  const segs = timelineSegments(sch);
  const savedLimitText = sch.limit_bps != null ? String(sch.limit_bps / MB) : "";

  const commitLimit = () => {
    const raw = limitText.trim();
    const n = Number(raw.replace(",", "."));
    if (raw !== "" && Number.isFinite(n) && n > 0) {
      const bps = Math.round(n * MB);
      if (bps !== sch.limit_bps) save({ ...sch, limit_bps: bps });
    } else {
      setLimitText(savedLimitText);
    }
  };
  const toggleLimit = (on: boolean) => {
    if (!on) return save({ ...sch, limit_bps: null });
    const n = Number(limitText.trim().replace(",", "."));
    const mb = Number.isFinite(n) && n > 0 ? n : 5;
    setLimitText(String(mb));
    save({ ...sch, limit_bps: Math.round(mb * MB) });
  };

  return (
    <div className="flex flex-col gap-4 p-5 max-w-[720px]">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-[16px] font-semibold text-ink">{t("sch.title")}</h1>
          <p className="text-[12.5px] text-ink-soft">
            {sch.enabled && label ? t("sch.next", { when: label, n: heldCount }) : t("sch.off")}
          </p>
        </div>
        <div className="flex items-center gap-2 text-[13px] text-ink">
          <span>{t("sch.enabled")}</span>
          <Switch checked={sch.enabled} label={t("sch.enabled")} onChange={(v) => save({ ...sch, enabled: v })} />
        </div>
      </div>

      {error ? <p role="alert" className="text-[12.5px] text-err">{error}</p> : null}

      <section className={CARD}>
        <h2 className="text-[13px] font-semibold text-ink">{t("sch.when")}</h2>
        <div className="flex items-center gap-2 text-[13px] text-ink-soft">
          <span>{t("sch.startAt")}</span>
          <input type="time" aria-label={t("sch.startAt")} className={FIELD} value={sch.start}
            onChange={(e) => e.target.value && save({ ...sch, start: e.target.value })} />
          <span>{t("sch.until")}</span>
          <input type="time" aria-label={t("sch.until")} className={FIELD} value={sch.end}
            onChange={(e) => e.target.value && save({ ...sch, end: e.target.value })} />
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[12.5px] text-ink-soft mr-1">{t("sch.days")}</span>
          {DAYS.map((d) => (
            <button key={d} type="button" aria-label={t(`day.${d}` as MsgKey)} aria-pressed={sch.days[d]}
              onClick={() => save({ ...sch, days: sch.days.map((v, i) => (i === d ? !v : v)) })}
              className={`w-8 h-8 rounded-full text-[12px] font-medium border transition-colors ${sch.days[d] ? "bg-accent text-accent-fg border-accent" : "bg-field text-ink-soft border-border"}`}>
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
      </section>

      <section className={CARD}>
        <h2 className="text-[13px] font-semibold text-ink">{t("sch.during")}</h2>
        <div className="flex items-center gap-3 text-[13px] text-ink">
          <Switch checked={limitOn} label={t("sch.limit")} onChange={toggleLimit} />
          <span>{t("sch.limit")}</span>
          <input type="text" inputMode="decimal" aria-label="Mo/s" className={`${FIELD} w-24`} disabled={!limitOn}
            value={limitText} onChange={(e) => setLimitText(e.target.value)} onBlur={commitLimit}
            onKeyDown={(e) => { if (e.key === "Enter") commitLimit(); }} />
          <span className="text-ink-soft">Mo/s</span>
        </div>
        <p className="text-[12px] text-ink-mute">{t("sch.limitHint")}</p>
        <label className="flex items-center gap-2 text-[13px] text-ink">
          <span>{t("sch.atEnd", { end: sch.end })}</span>
          <select className={FIELD} value={sch.at_end}
            onChange={(e) => save({ ...sch, at_end: e.target.value as Schedule["at_end"] })}>
            <option value="pause">{t("sch.atEndPause")}</option>
            <option value="continue">{t("sch.atEndContinue")}</option>
          </select>
        </label>
      </section>

      <section className={CARD}>
        <h2 className="text-[13px] font-semibold text-ink">{t("sch.done")}</h2>
        <label className="flex items-center gap-2 text-[13px] text-ink">
          <span>{t("sch.action")}</span>
          <select aria-label={t("sch.action")} className={FIELD} value={endAction ?? ""} disabled={endAction === null}
            onChange={(e) => {
              const a = e.target.value as EndAction;
              setEnd(a);
              setError(undefined);
              api.setEndAction(a).catch((er) => setError(String(er)));
            }}>
            {endAction === null ? <option value="" /> : null}
            {ACTIONS.map((a) => <option key={a} value={a}>{t(`end.${a}` as MsgKey)}</option>)}
          </select>
        </label>
        <p className="text-[12px] text-ink-mute">{t("sch.countdownNote")}</p>
      </section>
    </div>
  );
}
