import { useEffect, useMemo, useRef, useState } from "react";
import { Search, Trash2, X } from "lucide-react";
import * as api from "../api";
import { t } from "../i18n";
import type { Category, Download } from "../types";
import { filterLibrary, groupByDay } from "../library/library";
import { CategoryChips } from "../components/library/CategoryChips";
import { ViewToggle, type LibraryView } from "../components/library/ViewToggle";
import { LibraryList } from "../components/library/LibraryList";
import { LibraryGrid } from "../components/library/LibraryGrid";
import { DetailPanel } from "../components/library/DetailPanel";
import { DeleteDialog } from "../components/library/DeleteDialog";

const VIEW_PREF = "ui.completedView";

const errText = (e: unknown) => (typeof e === "string" ? e : String((e as { message?: unknown })?.message ?? e));

/**
 * Sélection après un changement de l'ordre affiché : l'élément reste s'il est encore là ;
 * sinon le suivant (dans l'ancien ordre) encore affiché, sinon le précédent, sinon aucun.
 */
function resolveSelection(id: string | null, prev: string[], next: string[]): string | null {
  if (id == null) return null;
  const present = new Set(next);
  if (present.has(id)) return id;
  const i = prev.indexOf(id);
  if (i < 0) return null;
  for (let j = i + 1; j < prev.length; j++) if (present.has(prev[j])) return prev[j];
  for (let j = i - 1; j >= 0; j--) if (present.has(prev[j])) return prev[j];
  return null;
}

type Selection = { id: string | null; order: string[]; key: string };

export function CompletedPage({
  downloads, onClear, onOpenFile, onOpenFolder, onCopyLink, onRedownload, onDeleted,
}: {
  downloads: Download[];
  onClear(): void;
  onOpenFile(id: string): void;
  onOpenFolder(id: string): void;
  onCopyLink(d: Download): void;
  onRedownload(url: string): void;
  onDeleted(): void;
}) {
  // null tant que la préférence n'est pas lue : évite d'afficher la liste puis la grille.
  const [view, setView] = useState<LibraryView | null>(null);
  const viewChosen = useRef(false);
  const [category, setCategory] = useState<Category | "all">("all");
  const [query, setQuery] = useState("");
  const [deleting, setDeleting] = useState<Download | null>(null);
  const [busy, setBusy] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  useEffect(() => {
    let stale = false;
    api.getUiPref(VIEW_PREF).then(
      (v) => { if (!stale && !viewChosen.current) setView(v === "grid" ? "grid" : "list"); },
      () => { if (!stale && !viewChosen.current) setView("list"); },
    );
    return () => { stale = true; };
  }, []);

  function changeView(v: LibraryView) {
    viewChosen.current = true;
    setView(v);
    api.setUiPref(VIEW_PREF, v).catch(() => {});
  }

  // Catégorie disparue (dernier élément supprimé) : retour à « Tout ».
  const hasCategory =
    category === "all" || downloads.some((d) => d.status === "completed" && d.category === category);
  if (!hasCategory) setCategory("all");

  const groups = useMemo(
    () => groupByDay(filterLibrary(downloads, category, query), new Date()),
    [downloads, category, query],
  );
  const shown = useMemo(() => groups.flatMap((g) => g.items), [groups]);
  const order = useMemo(() => shown.map((d) => d.id), [shown]);
  const orderKey = order.join("\n");

  // Sélection ajustée pendant le rendu quand l'ordre affiché change : pas d'image intermédiaire
  // montrant un panneau vide ou celui d'un élément disparu.
  const [sel, setSel] = useState<Selection>({ id: null, order: [], key: "" });
  let selectedId = sel.id;
  if (sel.key !== orderKey) selectedId = resolveSelection(sel.id, sel.order, order);
  if (selectedId == null && view === "list" && order.length > 0) selectedId = order[0];
  if (selectedId !== sel.id || sel.key !== orderKey) setSel({ id: selectedId, order, key: orderKey });
  const select = (id: string | null) => setSel({ id, order, key: orderKey });

  const selected = selectedId == null ? null : shown.find((d) => d.id === selectedId) ?? null;

  function askDelete(d: Download) {
    setDeleteError(null);
    setDeleting(d);
  }

  function cancelDelete() {
    setDeleting(null);
    setDeleteError(null);
  }

  async function runDelete(deleteFile: boolean) {
    if (!deleting) return;
    setBusy(true);
    setDeleteError(null);
    try {
      await api.deleteDownload(deleting.id, deleteFile);
    } catch (e) {
      setBusy(false);
      setDeleteError(errText(e));
      return;
    }
    setBusy(false);
    setDeleting(null);
    onDeleted();
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex flex-col gap-3 px-5 pt-5 pb-3">
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
          <ViewToggle value={view ?? "list"} onChange={changeView} />
          <button
            type="button"
            onClick={onClear}
            className="inline-flex items-center gap-1.5 h-9 px-3 rounded-lg border border-border bg-field text-ink text-[13px] font-medium hover:bg-surface"
          >
            <Trash2 size={15} />
            {t("done.clear")}
          </button>
        </div>
        <CategoryChips items={downloads} value={category} onChange={setCategory} />
      </div>
      <div className="flex flex-1 min-h-0">
        <div className="flex-1 min-w-0 overflow-auto px-5 pb-5">
          {view == null ? null : shown.length === 0 ? (
            <div className="p-8 text-center text-ink-mute text-sm">{downloads.some((d) => d.status === "completed") ? t("lib.noResults") : t("done.empty")}</div>
          ) : view === "list" ? (
            <LibraryList groups={groups} selectedId={selectedId} onSelect={select} />
          ) : (
            <LibraryGrid items={shown} selectedId={selectedId} onSelect={select} />
          )}
        </div>
        {view != null && selected && (
          <div className="relative flex shrink-0">
            <DetailPanel
              key={selected.id}
              d={selected}
              onOpenFile={() => onOpenFile(selected.id)}
              onOpenFolder={() => onOpenFolder(selected.id)}
              onCopyLink={() => onCopyLink(selected)}
              onRedownload={() => onRedownload(selected.url)}
              onDelete={() => askDelete(selected)}
            />
            {view === "grid" && (
              <button
                type="button"
                aria-label={t("lib.closeDetail")}
                title={t("lib.closeDetail")}
                onClick={() => select(null)}
                className="absolute top-2 right-2 inline-flex items-center justify-center w-7 h-7 rounded-md bg-black/60 text-white hover:bg-black/80"
              >
                <X size={14} />
              </button>
            )}
          </div>
        )}
      </div>
      {deleting && (
        <DeleteDialog
          name={deleting.file_name}
          busy={busy}
          error={deleteError}
          onRemove={() => runDelete(false)}
          onDeleteFile={() => runDelete(true)}
          onCancel={cancelDelete}
        />
      )}
    </div>
  );
}
