import { Rows3, LayoutGrid } from "lucide-react";
import { t } from "../../i18n";

export type LibraryView = "list" | "grid";

export function ViewToggle({ value, onChange }: { value: LibraryView; onChange: (v: LibraryView) => void }) {
  const btn = (v: LibraryView, label: string, icon: React.ReactNode) => (
    <button
      type="button"
      aria-label={label}
      title={label}
      aria-pressed={value === v}
      onClick={() => onChange(v)}
      className={`inline-flex items-center justify-center w-7 h-7 rounded ${
        value === v ? "bg-surface text-ink" : "text-ink-mute hover:text-ink"
      }`}
    >
      {icon}
    </button>
  );
  return (
    <div className="inline-flex gap-0.5 p-0.5 rounded-lg bg-field border border-line">
      {btn("list", t("lib.viewList"), <Rows3 size={15} />)}
      {btn("grid", t("lib.viewGrid"), <LayoutGrid size={15} />)}
    </div>
  );
}
