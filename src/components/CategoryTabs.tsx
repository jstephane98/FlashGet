import type { Category } from "../types";

const TABS: { key: Category | "all"; label: string }[] = [
  { key: "all", label: "Tous" },
  { key: "video", label: "🎬 Vidéos" },
  { key: "document", label: "📄 Documents" },
  { key: "program", label: "⚙ Programmes" },
  { key: "archive", label: "🗜 Archives" },
  { key: "music", label: "🎵 Musique" },
  { key: "other", label: "Autre" },
];

export function CategoryTabs({
  active,
  onChange,
}: {
  active: Category | "all";
  onChange: (c: Category | "all") => void;
}) {
  return (
    <div className="flex gap-1 px-3 pt-2 bg-gray-50 border-b border-gray-200 text-[11px]">
      {TABS.map((t) => (
        <button
          key={t.key}
          onClick={() => onChange(t.key)}
          className={`px-3 py-1.5 rounded-t-md ${
            active === t.key ? "bg-white text-indigo-600 font-semibold border border-gray-200 border-b-white -mb-px" : "text-gray-500"
          }`}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}
