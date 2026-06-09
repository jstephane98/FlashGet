import { LayoutGrid, Film, FileText, MonitorCog, FileArchive, Music, Folder, type LucideIcon } from "lucide-react";
import type { Category } from "../types";

const MAP: Record<Category | "all", LucideIcon> = {
  all: LayoutGrid,
  video: Film,
  document: FileText,
  program: MonitorCog,
  archive: FileArchive,
  music: Music,
  other: Folder,
};

export function CategoryIcon({ cat, size = 16, className }: { cat: Category | "all"; size?: number; className?: string }) {
  const Icon = MAP[cat];
  return <Icon size={size} className={className} />;
}
