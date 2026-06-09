import type { Category } from "../types";
import { CategoryIcon } from "./categoryIcons";

const TABS: { key: Category | "all"; label: string }[] = [
  { key: "all", label: "Tous" },
  { key: "video", label: "Vidéos" },
  { key: "document", label: "Documents" },
  { key: "program", label: "Programmes" },
  { key: "archive", label: "Archives" },
  { key: "music", label: "Musique" },
  { key: "other", label: "Autre" },
];

export function CategoryTabs({
  active,
  onChange,
}: {
  active: Category | "all";
  onChange: (c: Category | "all") => void;
}) {
  return (
    <div className="flex gap-1 px-3 pt-2 bg-bg border-b border-border text-[11px]">
      {TABS.map((t) => (
        <button
          key={t.key}
          onClick={() => onChange(t.key)}
          className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-t-md ${
            active === t.key ? "bg-surface text-accent-bright font-semibold border border-border border-b-surface -mb-px" : "text-ink-soft"
          }`}
        >
          <CategoryIcon cat={t.key} size={15} />
          {t.label}
        </button>
      ))}
    </div>
  );
}
