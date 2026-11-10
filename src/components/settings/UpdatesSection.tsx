import { t } from "../../i18n";
import { Card, SectionShell, SettingRow } from "./controls";
import { useVersions } from "./versions";

export function UpdatesSection() {
  const { app, ytdlp } = useVersions();
  return (
    <SectionShell title={t("set.nav.updates")} desc={t("set.updates.desc")}>
      <Card title={t("set.versionsCard")}>
        <SettingRow label={t("set.appName")}>
          {app ? <span className="font-mono text-[13px] text-ink">{app}</span> : null}
        </SettingRow>
        <SettingRow label="yt-dlp">
          {ytdlp ? <span className="font-mono text-[13px] text-ink">{ytdlp}</span>
            : ytdlp === null ? <span className="text-[13px] text-ink-mute">{t("set.ytdlpUnknown")}</span> : null}
        </SettingRow>
      </Card>
      <p className="text-[13px] text-ink-mute">{t("set.updatesSoon")}</p>
    </SectionShell>
  );
}
