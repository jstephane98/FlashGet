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
  return <span className="text-[10px] text-gray-500 bg-gray-100 rounded px-1.5 py-0.5">{LABEL[value]}</span>;
}
