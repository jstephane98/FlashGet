import { t } from "../../i18n";
import { formatClock, formatNumericDate } from "../../format";

/** « aujourd'hui à HH:MM », « hier à HH:MM », sinon « JJ/MM/AAAA » (heure locale). */
export function formatLastCheck(sec: number, now: Date): string {
  const d = new Date(sec * 1000);
  const time = formatClock(d);
  const dayStart = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round((dayStart(now) - dayStart(d)) / 86_400_000);
  if (diffDays === 0) return t("set.upd.today", { time });
  if (diffDays === 1) return t("set.upd.yesterday", { time });
  return formatNumericDate(d);
}

/** Date de publication au format JJ/MM/AAAA ; renvoie le texte tel quel s'il n'est pas une date. */
export function formatPublished(raw: string): string {
  const d = new Date(raw);
  if (Number.isNaN(d.getTime())) return raw;
  return formatNumericDate(d);
}
