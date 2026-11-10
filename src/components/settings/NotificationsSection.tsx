import type { Settings } from "../../types";
import { t } from "../../i18n";
import { Card, SectionShell, SettingRow, Switch } from "./controls";

export function NotificationsSection({ settings, save, error }: { settings: Settings; save(p: Partial<Settings>): void; error?: string; rev?: number }) {
  return (
    <SectionShell title={t("set.nav.notifications")} desc={t("set.notifications.desc")} error={error}>
      <Card title={t("set.notifyCard")}>
        <SettingRow label={t("set.notifyComplete")} hint={t("set.notifyCompleteHint")}>
          <Switch checked={settings.notify_complete} label={t("set.notifyComplete")} onChange={(v) => save({ notify_complete: v })} />
        </SettingRow>
        <SettingRow label={t("set.notifyError")} hint={t("set.notifyErrorHint")}>
          <Switch checked={settings.notify_error} label={t("set.notifyError")} onChange={(v) => save({ notify_error: v })} />
        </SettingRow>
      </Card>
    </SectionShell>
  );
}
