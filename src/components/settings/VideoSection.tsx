import { useEffect, useState } from "react";
import * as api from "../../api";
import type { AudioFormat, Settings } from "../../types";
import { t } from "../../i18n";
import type { MsgKey } from "../../i18n";
import { Card, SectionShell, Select, SettingRow, ghostBtn } from "./controls";

const HEIGHTS = [2160, 1440, 1080, 720, 480, 360];
const FORMATS: { value: AudioFormat; label: string }[] = [
  { value: "mp3", label: "MP3" },
  { value: "m4a", label: "M4A" },
  { value: "opus", label: "Opus" },
];
// 0 = qualité VBR la plus haute de yt-dlp (--audio-quality 0).
const KBPS = [0, 320, 256, 192, 128];
const LANGS = ["fr", "en", "es", "de", "it", "pt", "ar", "ja"];

export function VideoSection({ settings, save, error }: { settings: Settings; save(p: Partial<Settings>): void; error?: string; rev?: number }) {
  const [version, setVersion] = useState<string | null | undefined>(undefined);
  const [ytConnected, setYtConnected] = useState(false);
  const [browser, setBrowser] = useState("");
  const [localError, setLocalError] = useState<string>();

  useEffect(() => {
    let alive = true;
    const refresh = () => api.youtubeStatus().then((v) => { if (alive) setYtConnected(v); }).catch(() => {});
    api.getYtdlpVersion().then((v) => { if (alive) setVersion(v); }).catch(() => { if (alive) setVersion(null); });
    api.getCookiesFromBrowser().then((b) => { if (alive) setBrowser(b ?? ""); }).catch(() => {});
    refresh();
    const unsub = api.onYoutubeStatus(refresh);
    return () => { alive = false; unsub.then((f) => f()).catch(() => {}); };
  }, []);

  const heights = HEIGHTS.includes(settings.video_max_height) ? HEIGHTS : [settings.video_max_height, ...HEIGHTS];
  const kbps = KBPS.includes(settings.audio_kbps) ? KBPS : [settings.audio_kbps, ...KBPS];

  const toggleLang = (lang: string, on: boolean) => {
    const cur = settings.sub_langs;
    if (on) {
      if (!cur.includes(lang)) save({ sub_langs: [...cur, lang] });
      return;
    }
    if (cur.length <= 1) return; // au moins une langue reste cochée
    save({ sub_langs: cur.filter((l) => l !== lang) });
  };

  const run = (p: Promise<unknown>) => {
    setLocalError(undefined);
    p.catch((e) => setLocalError(String(e)));
  };
  const logout = () => run(api.youtubeLogout().then(() => setYtConnected(false)));
  const pickBrowser = (v: string) => {
    const prev = browser;
    setBrowser(v);
    setLocalError(undefined);
    api.setCookiesFromBrowser(v || null).catch((e) => { setBrowser(prev); setLocalError(String(e)); });
  };

  const browsers = [
    { value: "none", label: t("set.cookiesNone") },
    { value: "chrome", label: "Chrome" },
    { value: "edge", label: "Edge" },
    { value: "firefox", label: "Firefox" },
  ];

  return (
    <SectionShell title={t("set.nav.video")} desc={t("set.video.desc")} error={error ?? localError}>
      <Card title={t("set.engineCard")}>
        <SettingRow label={t("set.engineCard")}>
          {version ? (
            <span className="text-[13px] text-ink-soft">yt-dlp <span className="font-mono text-ink">{version}</span></span>
          ) : version === null ? (
            <span className="text-[13px] text-ink-mute">{t("set.ytdlpUnknown")}</span>
          ) : null}
        </SettingRow>
      </Card>
      <Card title={t("set.formatsCard")}>
        <SettingRow label={t("set.quality")} hint={t("set.qualityHint")}>
          <Select label={t("set.quality")} value={String(settings.video_max_height)}
            options={heights.map((h) => ({ value: String(h), label: `${h}p` }))}
            onChange={(v) => save({ video_max_height: Number(v) })} />
        </SettingRow>
        <SettingRow label={t("set.audioFormat")}>
          <div role="group" aria-label={t("set.audioFormat")} className="inline-flex gap-1">
            {FORMATS.map((f) => (
              <button key={f.value} type="button" aria-pressed={settings.audio_format === f.value}
                onClick={() => { if (settings.audio_format !== f.value) save({ audio_format: f.value }); }}
                className={`h-8 px-3 rounded-lg border text-[13px] font-semibold outline-none focus-visible:ring-1 focus-visible:ring-accent/50 ${settings.audio_format === f.value ? "bg-accent text-accent-ink border-accent" : "bg-field text-ink-soft border-border hover:bg-surface"}`}>
                {f.label}
              </button>
            ))}
          </div>
          <Select label={t("set.audioKbps")} value={String(settings.audio_kbps)}
            options={kbps.map((k) => ({
              value: String(k),
              label: k === 0 ? t("set.kbpsMax") : t("set.kbpsUnit", { n: k }),
            }))}
            onChange={(v) => save({ audio_kbps: Number(v) })} />
        </SettingRow>
        <SettingRow label={t("set.subs")} hint={t("set.subsHint")}>
          <div role="group" aria-label={t("set.subs")} className="flex flex-wrap justify-end gap-x-4 gap-y-1.5 max-w-[320px]">
            {LANGS.map((l) => {
              const label = t(`set.lang.${l}` as MsgKey);
              return (
                <label key={l} className="inline-flex items-center gap-1.5 text-[13px] text-ink-soft">
                  <input type="checkbox" aria-label={label} className="accent-accent"
                    checked={settings.sub_langs.includes(l)}
                    onChange={(e) => toggleLang(l, e.target.checked)} />
                  {label}
                </label>
              );
            })}
          </div>
        </SettingRow>
      </Card>
      <Card title={t("set.accountsCard")}>
        <SettingRow label={t("set.ytAccount")} hint={t("set.ytAccountHint")}>
          <span className={`inline-flex items-center gap-1.5 text-[12px] font-semibold ${ytConnected ? "text-ok" : "text-ink-mute"}`}>
            <span className={`inline-block w-2 h-2 rounded-full ${ytConnected ? "bg-ok" : "bg-ink-mute"}`} />
            {ytConnected ? t("set.connected") : t("set.disconnected")}
          </span>
          {ytConnected ? (
            <button type="button" className={ghostBtn} onClick={logout}>{t("set.ytLogout")}</button>
          ) : (
            <button type="button" className="h-8 px-3 rounded-lg cta-grad text-[13px] font-semibold outline-none focus-visible:ring-1 focus-visible:ring-accent/50"
              onClick={() => run(api.youtubeLogin())}>{t("set.ytLogin")}</button>
          )}
        </SettingRow>
        <SettingRow label={t("set.cookies")} hint={t("set.cookiesHint")}>
          <Select label={t("set.cookies")} value={browser || "none"} options={browsers}
            onChange={(v) => pickBrowser(v === "none" ? "" : v)} />
        </SettingRow>
      </Card>
    </SectionShell>
  );
}
