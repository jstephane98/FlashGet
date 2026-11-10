import { useMemo, useState } from "react";
import { Search, Trash2 } from "lucide-react";
import { t } from "../i18n";
import type { Download } from "../types";
import { DownloadCard } from "../components/downloads/DownloadCard";
import type { CardActions } from "./DownloadsPage";

// Page provisoire jusqu'à la phase 3.
export function CompletedPage({
  downloads, onClear, actions,
}: {
  downloads: Download[];
  onClear(): void;
  actions: CardActions;
}) {
  const [query, setQuery] = useState("");
  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    return downloads
      .filter((d) => d.status === "completed" && (!q || d.file_name.toLowerCase().includes(q)))
      .reverse();
  }, [downloads, query]);

  return (
    <div className="flex flex-col h-full p-5 gap-4">
      <div className="flex items-center gap-3">
        <h1 className="text-[20px] font-extrabold text-ink mr-auto">{t("done.title")}</h1>
        <div className="relative w-60">
          <Search size={15} className="absolute left-3 top-1/2 -translate-y-1/2 text-ink-mute" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("dl.search")}
            className="w-full h-9 pl-9 pr-3 rounded-lg border border-border bg-field text-ink text-[13px] placeholder:text-ink-mute outline-none focus:border-accent focus:ring-1 focus:ring-accent/50"
          />
        </div>
        <button
          type="button"
          onClick={onClear}
          className="inline-flex items-center gap-1.5 h-9 px-3 rounded-lg border border-border bg-field text-ink text-[13px] font-medium hover:bg-surface"
        >
          <Trash2 size={15} />
          {t("done.clear")}
        </button>
      </div>
      <div className="flex-1 overflow-auto flex flex-col gap-2">
        {rows.length === 0 ? (
          <div className="p-8 text-center text-ink-mute text-sm">{t("done.empty")}</div>
        ) : (
          rows.map((d) => (
            <DownloadCard
              key={d.id}
              d={d}
              speed={0}
              queuePosition={null}
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
