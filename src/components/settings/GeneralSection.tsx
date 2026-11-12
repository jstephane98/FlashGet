import { useEffect, useState } from "react";
import { Moon, Sun, Monitor } from "lucide-react";
import * as api from "../../api";
import type { Settings } from "../../types";
import { t, LANG_NAMES } from "../../i18n";
import type { LangPref, MsgKey } from "../../i18n";
import { useLanguage } from "../../i18n/LanguageProvider";
import { useTheme } from "../../theme/ThemeProvider";
import type { ThemePref } from "../../theme/theme";
import { Card, SectionShell, Select, SettingRow, Switch } from "./controls";

const THEMES: { pref: ThemePref; icon: typeof Moon }[] = [
  { pref: "dark", icon: Moon },
  { pref: "light", icon: Sun },
  { pref: "system", icon: Monitor },
];

export function GeneralSection({ settings, save, error }: { settings: Settings; save(p: Partial<Settings>): void; error?: string }) {
  const { pref, setPref } = useTheme();
  const language = useLanguage();
  const langOptions: { value: LangPref; label: string }[] = [
    { value: "auto", label: t("set.languageAutoWith", { lang: LANG_NAMES[language.system] }) },
    { value: "fr", label: LANG_NAMES.fr },
    { value: "en", label: LANG_NAMES.en },
  ];
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [localError, setLocalError] = useState<string>();

  useEffect(() => {
    let alive = true;
    api.getAutostart().then((v) => { if (alive) setAutostart(v); }).catch(() => { if (alive) setAutostart(false); });
    return () => { alive = false; };
  }, []);

  const toggleAutostart = (v: boolean) => {
    setLocalError(undefined);
    setAutostart(v);
    api.setAutostart(v).catch((e) => { setAutostart(!v); setLocalError(String(e)); });
  };

  return (
    <SectionShell title={t("set.nav.general")} desc={t("set.general.desc")} error={error ?? localError}>
      <Card title={t("set.general.appearance")}>
        <SettingRow label={t("set.language")} hint={t("set.languageHint")}>
          <Select label={t("set.language")} value={language.pref} options={langOptions} onChange={language.setPref} />
        </SettingRow>
        <SettingRow label={t("set.theme")} hint={t("set.themeHint")}>
          <div className="inline-flex p-[3px] gap-0.5 rounded-lg bg-field border border-border" role="group" aria-label={t("set.theme")}>
            {THEMES.map(({ pref: p, icon: Icon }) => (
              <button key={p} type="button" aria-pressed={pref === p} onClick={() => setPref(p)}
                className={`inline-flex items-center gap-1.5 px-[11px] py-[5px] rounded-md text-[12px] font-semibold ${pref === p ? "bg-surface text-ink" : "text-ink-mute"}`}>
                <Icon size={13} aria-hidden />
                {t(`theme.${p}` as MsgKey)}
              </button>
            ))}
          </div>
        </SettingRow>
      </Card>
      <Card title={t("set.general.startup")}>
        <SettingRow label={t("set.autostart")}>
          <Switch checked={autostart ?? false} disabled={autostart === null} label={t("set.autostart")} onChange={toggleAutostart} />
        </SettingRow>
        <SettingRow label={t("set.closeToTray")} hint={t("set.closeToTrayHint")}>
          <Switch checked={settings.close_to_tray} label={t("set.closeToTray")} onChange={(v) => save({ close_to_tray: v })} />
        </SettingRow>
      </Card>
    </SectionShell>
  );
}
