import { NAV_ITEMS, type NavItem, type PageId } from "../../navigation";
import { t } from "../../i18n";

interface Props {
  active: PageId;
  counts: Partial<Record<PageId, number>>;
  onNavigate(id: PageId): void;
}

export function Sidebar({ active, counts, onNavigate }: Props) {
  const items = NAV_ITEMS.filter((i) => i.ready);
  const render = (item: NavItem) => {
    const Icon = item.icon;
    const count = counts[item.id];
    if (item.id === "new") {
      return (
        <button
          key={item.id}
          type="button"
          onClick={() => onNavigate(item.id)}
          className="mb-3 flex w-full items-center justify-center gap-2 rounded-lg bg-cta px-3 py-2 text-[13px] font-semibold text-cta-ink transition-opacity hover:opacity-90"
        >
          <Icon size={15} strokeWidth={2.5} />
          <span>{t(item.labelKey)}</span>
        </button>
      );
    }
    const isActive = item.id === active;
    return (
      <button
        key={item.id}
        type="button"
        aria-current={isActive ? "page" : undefined}
        onClick={() => onNavigate(item.id)}
        className={`flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-[13px] font-medium transition-colors ${
          isActive ? "bg-accent/10 text-accent-fg" : "text-ink-soft hover:bg-surface hover:text-ink"
        }`}
      >
        <Icon size={16} />
        <span className="flex-1 text-left">{t(item.labelKey)}</span>
        {count !== undefined && (
          <span className="rounded-full bg-surface px-1.5 font-mono text-[11px] text-ink-soft">{count}</span>
        )}
      </button>
    );
  };
  return (
    <nav className="flex w-[210px] shrink-0 flex-col border-r border-border bg-panel p-3 font-sans">
      <div className="flex flex-col gap-0.5">{items.filter((i) => !i.bottom).map(render)}</div>
      <div className="mt-auto flex flex-col gap-0.5">{items.filter((i) => i.bottom).map(render)}</div>
    </nav>
  );
}
