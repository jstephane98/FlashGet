import { t } from "../../i18n";
import { BrandMark } from "../shell/BrandMark";
import { SectionShell } from "./controls";
import { useVersions } from "./versions";

export function AboutSection() {
  const { app } = useVersions();
  return (
    <SectionShell title={t("set.nav.about")} desc={t("set.about.desc")}>
      <div className="rounded-[10px] border border-border bg-panel px-6 py-8 flex flex-col items-center text-center gap-2">
        <span role="img" aria-label={t("set.aboutLogo")} className="flex">
          <BrandMark size={64} />
        </span>
        <h3 className="text-[20px] font-extrabold text-ink">{t("set.appName")}</h3>
        {app ? <p className="font-mono text-[13px] text-ink-soft">{t("set.aboutVersion", { v: app })}</p> : null}
        <p className="text-[13px] text-ink-soft max-w-[360px]">{t("set.aboutTagline")}</p>
        <p className="text-[12px] text-ink-mute mt-2">{t("set.aboutComponents")}</p>
      </div>
    </SectionShell>
  );
}
