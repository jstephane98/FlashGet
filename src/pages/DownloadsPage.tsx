import { useMemo, useState } from "react";
import { Layers, Pause, Play, Plus, Search } from "lucide-react";
import { t } from "../i18n";
import type { Download, Priority } from "../types";
import { DownloadCard } from "../components/downloads/DownloadCard";
import { queuePositions, tabCounts, visibleDownloads, type DownloadTab } from "../downloads/tabs";

export type CardActions = {
  pause(id: string): void;
  resume(id: string): void;
  cancel(id: string): void;
  stop(id: string): void;
  priority(id: string, p: Priority): void;
  openFile(id: string): void;
  openFolder(id: string): void;
  remove(id: string): void;
};

const TABS: DownloadTab[] = ["all", "active", "paused", "error"];

const secondaryBtn =
  "inline-flex items-center gap-1.5 h-9 px-3 rounded-lg border border-border bg-field text-ink text-[13px] font-medium hover:bg-surface";

export function DownloadsPage({
  downloads, speeds, parts, segments, onAddLink, onAddBatch, onPauseAll, onResumeAll, actions,
}: {
  downloads: Download[];
  speeds: Record<string, number>;
  parts: Record<string, number[] | null>;
  segments: Record<string, { done: number; total: number }>;
  onAddLink(): void;
  onAddBatch(): void;
  onPauseAll(): void;
  onResumeAll(): void;
  actions: CardActions;
}) {
  const [tab, setTab] = useState<DownloadTab>("all");
  const [query, setQuery] = useState("");
  const counts = useMemo(() => tabCounts(downloads), [downloads]);
  const rows = useMemo(() => visibleDownloads(downloads, tab, query), [downloads, tab, query]);
  const positions = useMemo(() => queuePositions(downloads), [downloads]);
  const hasActive = counts.active > 0;

  return (
    <div className="flex flex-col h-full p-5 gap-4">
      <div className="flex items-center gap-3">
        <h1 className="text-[20px] font-extrabold text-ink mr-auto">{t("dl.title")}</h1>
        <div className="relative w-60">
          <Search size={15} className="absolute left-3 top-1/2 -translate-y-1/2 text-ink-mute" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("dl.search")}
            className="w-full h-9 pl-9 pr-3 rounded-lg border border-border bg-field text-ink text-[13px] placeholder:text-ink-mute outline-none focus:border-accent focus:ring-1 focus:ring-accent/50"
          />
        </div>
        <button type="button" className={secondaryBtn} onClick={onAddBatch}>
          <Layers size={15} />
          {t("dl.addBatch")}
        </button>
        {hasActive ? (
          <button type="button" className={secondaryBtn} onClick={onPauseAll}>
            <Pause size={15} />
            {t("dl.pauseAll")}
          </button>
        ) : (
          <button type="button" className={secondaryBtn} onClick={onResumeAll}>
            <Play size={15} />
            {t("dl.resumeAll")}
          </button>
        )}
        <button
          type="button"
          onClick={onAddLink}
          className="inline-flex items-center gap-1.5 h-9 px-4 rounded-lg bg-cta text-cta-ink text-[13px] font-semibold hover:opacity-90"
        >
          <Plus size={15} />
          {t("dl.addLink")}
        </button>
      </div>

      <div className="flex gap-1.5" role="tablist">
        {TABS.map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            onClick={() => setTab(id)}
            className={`h-8 px-3.5 rounded-full text-[13px] font-medium ${
              tab === id ? "border border-border bg-panel text-ink shadow-sm" : "text-ink-soft hover:text-ink"
            }`}
          >
            {t(`tab.${id}` as const, { n: counts[id] })}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-auto flex flex-col gap-2">
        {rows.length === 0 ? (
          <div className="p-8 text-center text-ink-mute text-sm">{t("dl.empty")}</div>
        ) : (
          rows.map((d) => (
            <DownloadCard
              key={d.id}
              d={d}
              speed={speeds[d.id] ?? 0}
              parts={parts[d.id]}
              seg={segments[d.id]}
              queuePosition={positions[d.id] ?? null}
              onPause={() => actions.pause(d.id)}
              onResume={() => actions.resume(d.id)}
              onCancel={() => actions.cancel(d.id)}
              onStop={() => actions.stop(d.id)}
              onPriority={(p) => actions.priority(d.id, p)}
              onOpenFile={() => actions.openFile(d.id)}
              onOpenFolder={() => actions.openFolder(d.id)}
              onRemove={() => actions.remove(d.id)}
            />
          ))
        )}
      </div>
    </div>
  );
}
