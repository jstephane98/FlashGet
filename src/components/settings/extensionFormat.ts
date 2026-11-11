import { t } from "../../i18n";

/** Dernière activité de l'extension : « à l'instant » sous une minute, sinon en minutes. */
export function formatLastSeen(lastSeen: number, now: number): string {
  const secs = now - lastSeen;
  if (secs < 60) return t("set.extSeenNow");
  return t("set.extSeenMin", { n: Math.floor(secs / 60) });
}
