import type { Category } from "../types";
import { CategoryIcon } from "./categoryIcons";

const LABEL: Record<Category, string> = {
  video: "Vidéos",
  document: "Docs",
  program: "Prog.",
  archive: "Archives",
  music: "Musique",
  other: "Autre",
};

export function CategoryPill({ value }: { value: Category }) {
  return (
    <span className="text-[10px] text-ink-soft bg-surface border border-border rounded px-1.5 py-0.5">
      <span className="inline-flex items-center gap-1"><CategoryIcon cat={value} size={13} /> {LABEL[value]}</span>
    </span>
  );
}
