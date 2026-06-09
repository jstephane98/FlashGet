import type { Category } from "../types";

const LABEL: Record<Category, string> = {
  video: "🎬 Vidéos",
  document: "📄 Docs",
  program: "⚙ Prog.",
  archive: "🗜 Archives",
  music: "🎵 Musique",
  other: "Autre",
};

export function CategoryPill({ value }: { value: Category }) {
  return <span className="text-[10px] text-ink-soft bg-surface border border-border rounded px-1.5 py-0.5">{LABEL[value]}</span>;
}
