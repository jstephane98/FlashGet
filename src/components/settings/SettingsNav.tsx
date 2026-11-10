import { Sliders, Gauge, Folder, Film, Puzzle, Bell, RefreshCw, Info, type LucideIcon } from "lucide-react";
import { t } from "../../i18n";
import type { MsgKey } from "../../i18n";

export type SectionId = "general" | "downloads" | "folders" | "video" | "extension" | "notifications" | "updates" | "about";

export const SECTIONS: { id: SectionId; icon: LucideIcon }[] = [
  { id: "general", icon: Sliders },
  { id: "downloads", icon: Gauge },
  { id: "folders", icon: Folder },
  { id: "video", icon: Film },
  { id: "extension", icon: Puzzle },
  { id: "notifications", icon: Bell },
  { id: "updates", icon: RefreshCw },
  { id: "about", icon: Info },
];

export function SettingsNav({ active, onSelect }: { active: SectionId; onSelect(id: SectionId): void }) {
  return (
    <nav aria-label={t("set.title")} className="w-[200px] shrink-0 border-r border-line px-2.5 py-[18px] flex flex-col gap-0.5">
      <h1 className="text-[20px] font-extrabold text-ink mx-2.5 mb-3.5">{t("set.title")}</h1>
      {SECTIONS.map(({ id, icon: Icon }) => (
        <button key={id} type="button" onClick={() => onSelect(id)} aria-current={active === id ? "page" : undefined}
          className={`flex items-center gap-[9px] px-2.5 py-[7px] rounded-[7px] text-[13px] font-medium text-left outline-none focus-visible:ring-1 focus-visible:ring-accent/50 ${active === id ? "bg-surface text-ink" : "text-ink-soft hover:bg-surface/60"}`}>
          <Icon size={16} aria-hidden />
          {t(`set.nav.${id}` as MsgKey)}
        </button>
      ))}
    </nav>
  );
}
