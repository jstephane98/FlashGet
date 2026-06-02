export function Toolbar({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-200 bg-white">
      <button
        className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold"
        onClick={onAdd}
      >
        + Ajouter
      </button>
      <div className="flex-1" />
      <input
        className="bg-gray-100 rounded-md px-3 py-1.5 text-sm text-gray-500 w-48"
        placeholder="🔍 Rechercher…"
        disabled
        title="Recherche disponible en Phase 2"
      />
    </div>
  );
}
