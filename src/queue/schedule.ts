import type { Schedule } from "../api";
import { t } from "../i18n";
import type { MsgKey } from "../i18n";

/** "HH:MM" strict -> minutes 0..1439. */
export function parseHHMM(s: string): number | null {
  const m = /^(\d{2}):(\d{2})$/.exec(s);
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  return h > 23 || min > 59 ? null : h * 60 + min;
}

/** `weekday` : 0 = lundi. Une plage qui passe minuit appartient au jour où elle commence. */
export function inWindow(s: Schedule, weekday: number, minute: number): boolean {
  if (!s.enabled || weekday < 0 || weekday > 6) return false;
  const start = parseHHMM(s.start);
  const end = parseHHMM(s.end);
  if (start == null || end == null) return false;
  const today = s.days[weekday];
  if (start < end) return today && minute >= start && minute < end;
  if (start > end) {
    const yesterday = s.days[(weekday + 6) % 7];
    return (today && minute >= start) || (yesterday && minute < end);
  }
  return false;
}

/** Premier début strictement après maintenant : [jours d'écart 0..7, minute]. */
export function nextStart(s: Schedule, weekday: number, minute: number): [number, number] | null {
  if (!s.enabled || weekday < 0 || weekday > 6) return null;
  const start = parseHHMM(s.start);
  if (start == null) return null;
  for (let d = 0; d <= 7; d++) {
    if (s.days[(weekday + d) % 7] && (d > 0 || start > minute)) return [d, start];
  }
  return null;
}

/** Plage horaire en pourcentages de 24 h ; deux segments si elle passe minuit. */
export function timelineSegments(s: Schedule): { left: number; width: number }[] {
  const start = parseHHMM(s.start);
  const end = parseHHMM(s.end);
  if (start == null || end == null || start === end) return [];
  const pct = (m: number) => (m / 1440) * 100;
  if (start < end) return [{ left: pct(start), width: pct(end - start) }];
  return [
    { left: pct(start), width: pct(1440 - start) },
    { left: 0, width: pct(end) },
  ].filter((seg) => seg.width > 0);
}

export function nextStartLabel(s: Schedule, now: Date): string | null {
  const weekday = (now.getDay() + 6) % 7;
  const minute = now.getHours() * 60 + now.getMinutes();
  const next = nextStart(s, weekday, minute);
  if (!next) return null;
  const [days, start] = next;
  const time = s.start;
  const delta = days * 1440 + start - minute;
  const tonight =
    (days === 0 && (start >= 20 * 60 || start < 6 * 60)) || (days === 1 && start < 6 * 60 && delta < 1440);
  if (tonight) return t("sch.tonight", { time });
  if (days === 0) return t("sch.today", { time });
  if (days === 1) return t("sch.tomorrow", { time });
  return t("sch.onDay", { day: t(`day.${(weekday + days) % 7}` as MsgKey), time });
}
