import { useMemo, useState } from "react";
import { ChevronDown, ChevronUp, GripVertical, Pause, Play, X } from "lucide-react";
import { t } from "../i18n";
import * as api from "../api";
import type { Schedule } from "../api";
import type { Download, Priority } from "../types";
import { Thumb } from "../components/library/Thumb";
import { formatBytes, hostOf } from "../format";
import { applyDrop, queueItems, queueSummary } from "../queue/queue";

const secondaryBtn =
  "inline-flex items-center gap-1.5 h-9 px-3 rounded-lg border border-border bg-field text-ink text-[13px] font-medium hover:bg-surface disabled:opacity-50";
const iconBtn =
  "h-7 w-7 inline-flex items-center justify-center rounded-md border border-border bg-field text-ink-soft hover:text-ink hover:bg-surface disabled:opacity-40";

const PRIOS: Priority[] = ["high", "normal", "low"];

export function QueuePage({
  downloads, maxConcurrent, schedule, paused, onChanged,
}: {
  downloads: Download[];
  maxConcurrent: number;
  schedule: Schedule | null;
  paused: boolean;
  onChanged(): void;
}) {
  const items = useMemo(() => queueItems(downloads), [downloads]);
  const summary = useMemo(() => queueSummary(items), [items]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [overIdx, setOverIdx] = useState<number | null>(null);

  async function run(action: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      onChanged();
    }
  }

  function move(from: number, to: number) {
    if (to < 0 || to >= items.length) return;
    const { order, moved, priority } = applyDrop(items, from, to);
    const current = items[from].priority;
    void run(async () => {
      if (from !== to && priority !== current) await api.setPriority(moved, priority);
      await api.reorderQueue(order);
    });
  }

  function endDrag() {
    setDragFrom(null);
    setOverIdx(null);
  }

  const schedActive = !!schedule?.enabled;

  return (
    <div className="flex flex-col h-full p-5 gap-4">
      <div className="flex items-center gap-3">
        <div className="mr-auto min-w-0">
          <div className="flex items-center gap-2">
            <h1 className="text-[20px] font-extrabold text-ink">{t("q.title")}</h1>
            {paused && (
              <span className="px-2 py-0.5 rounded-full bg-surface text-accent-fg text-[11px] font-semibold">
                {t("q.pausedBadge")}
              </span>
            )}
          </div>
          <p className="text-[12.5px] text-ink-soft font-mono">
            {t("q.summary", {
              n: summary.count,
              size: summary.bytes == null ? "—" : formatBytes(summary.bytes),
              max: maxConcurrent,
            })}
          </p>
        </div>
        <button type="button" className={secondaryBtn} disabled={busy} onClick={() => void run(() => api.pauseQueue())}>
          <Pause size={15} />
          {t("q.pause")}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => void run(() => api.startQueue())}
          className="inline-flex items-center gap-1.5 h-9 px-4 rounded-lg bg-cta text-cta-ink text-[13px] font-semibold hover:opacity-90 disabled:opacity-50"
        >
          <Play size={15} />
          {t("q.startAll")}
        </button>
      </div>

      {error && (
        <div role="alert" className="text-[12.5px] text-err">
          {error}
        </div>
      )}

      <div className="flex-1 overflow-auto flex flex-col gap-2">
        {items.length === 0 ? (
          <div className="p-8 text-center text-ink-mute text-sm">{t("q.empty")}</div>
        ) : (
          items.map((d, i) => (
            <div key={d.id} className="relative">
              {overIdx === i && dragFrom !== null && dragFrom !== i && (
                <div
                  className={`absolute left-0 right-0 h-0.5 bg-accent rounded ${dragFrom > i ? "-top-1.5" : "-bottom-1.5"}`}
                />
              )}
              <div
                data-testid="queue-row"
                draggable
                onDragStart={(e) => {
                  e.dataTransfer?.setData("text/plain", d.id);
                  if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
                  setDragFrom(i);
                }}
                onDragOver={(e) => {
                  if (dragFrom === null) return;
                  e.preventDefault();
                  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
                  setOverIdx(i);
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  const from = dragFrom;
                  endDrag();
                  if (from !== null && from !== i) move(from, i);
                }}
                onDragEnd={endDrag}
                className={`flex items-center gap-3 p-2.5 rounded-xl border bg-panel ${
                  dragFrom === i ? "border-accent shadow-lg opacity-80" : "border-line"
                }`}
              >
                <GripVertical size={16} className="text-ink-mute cursor-grab shrink-0" />
                <span className="w-6 h-6 rounded-full bg-surface text-ink-soft text-[12px] font-mono font-semibold inline-flex items-center justify-center shrink-0">
                  {i + 1}
                </span>
                <Thumb d={d} className="w-14 h-9 shrink-0" />
                <div className="min-w-0 flex-1">
                  <div className="text-[13px] font-semibold text-ink truncate">{d.file_name}</div>
                  <div className="text-[11.5px] text-ink-mute truncate">{hostOf(d.url)}</div>
                </div>
                {d.status === "held" && (
                  <span className="text-[12px] text-accent-fg whitespace-nowrap">
                    {schedActive && schedule ? t("q.scheduled", { time: schedule.start }) : t("dl.held")}
                  </span>
                )}
                <select
                  aria-label={t("q.priority")}
                  value={d.priority}
                  disabled={busy}
                  onChange={(e) => void run(() => api.setPriority(d.id, e.target.value as Priority))}
                  className="h-7 rounded-md border border-border bg-field text-ink text-[12px] px-1.5"
                >
                  {PRIOS.map((p) => (
                    <option key={p} value={p}>
                      {t(`prio.${p}` as const)}
                    </option>
                  ))}
                </select>
                <span className="w-16 text-right text-[12px] text-ink-soft font-mono tabular-nums">
                  {d.total_bytes == null ? "—" : formatBytes(d.total_bytes)}
                </span>
                <button type="button" aria-label={t("q.up")} title={t("q.up")} className={iconBtn}
                  disabled={busy || i === 0} onClick={() => move(i, i - 1)}>
                  <ChevronUp size={14} />
                </button>
                <button type="button" aria-label={t("q.down")} title={t("q.down")} className={iconBtn}
                  disabled={busy || i === items.length - 1} onClick={() => move(i, i + 1)}>
                  <ChevronDown size={14} />
                </button>
                <button type="button" aria-label={t("q.remove")} title={t("q.remove")} className={iconBtn}
                  disabled={busy}
                  onClick={() => {
                    if (window.confirm(t("q.removeConfirm"))) void run(() => api.removeFromList(d.id));
                  }}>
                  <X size={14} />
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
