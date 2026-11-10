import type { Category, Download } from "../../types";
import { categoryCounts } from "../../library/library";
import { t } from "../../i18n";
import { CategoryIcon } from "../categoryIcons";

const CHIP = "inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs border";

export function CategoryChips({
  items,
  value,
  onChange,
}: {
  items: Download[];
  value: Category | "all";
  onChange: (v: Category | "all") => void;
}) {
  const counts = categoryCounts(items);
  const total = counts.reduce((n, c) => n + c.count, 0);
  const cls = (active: boolean) =>
    `${CHIP} ${active ? "bg-surface text-ink border-border" : "bg-transparent text-ink-soft border-line hover:text-ink"}`;
  return (
    <div className="flex flex-wrap gap-2">
      <button className={cls(value === "all")} aria-pressed={value === "all"} onClick={() => onChange("all")}>
        <CategoryIcon cat="all" size={13} />
        {t("lib.all", { n: total })}
      </button>
      {counts.map(({ category, count }) => (
        <button key={category} className={cls(value === category)} aria-pressed={value === category}
          onClick={() => onChange(category)}>
          <CategoryIcon cat={category} size={13} />
          {t(`catp.${category}` as const, { n: count })}
        </button>
      ))}
    </div>
  );
}
