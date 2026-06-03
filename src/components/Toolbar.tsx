export function Toolbar({
  onAdd,
  onPauseAll,
  onResumeAll,
  onSettings,
}: {
  onAdd: () => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onSettings: () => void;
}) {
  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-200 bg-white">
      <button
        className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold"
        onClick={onAdd}
      >
        + Ajouter
      </button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onPauseAll}>
        ⏸ Tout
      </button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onResumeAll}>
        ▶ Tout
      </button>
      <div className="flex-1" />
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onSettings} title="Réglages">
        ⚙
      </button>
    </div>
  );
}
