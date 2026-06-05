import { useState } from "react";

export function AddBatchDialog({
  open,
  onClose,
  onAdd,
}: {
  open: boolean;
  onClose: () => void;
  onAdd: (text: string) => void;
}) {
  const [text, setText] = useState("");
  if (!open) return null;

  function submit() {
    if (!text.trim()) return;
    onAdd(text);
    setText("");
  }

  return (
    <div className="fixed inset-0 bg-black/30 flex items-center justify-center" onClick={onClose}>
      <div className="bg-white rounded-lg p-5 w-[560px]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-semibold mb-1">Ajouter plusieurs liens</h3>
        <p className="text-xs text-gray-500 mb-3">
          Un lien par ligne. Seuls les liens http(s) directs sont ajoutés.
        </p>
        <textarea
          autoFocus
          aria-label="Liens à télécharger"
          className="w-full h-48 border border-gray-200 rounded-md px-3 py-2 text-sm font-mono resize-none"
          placeholder={"https://exemple.com/fichier1.zip\nhttps://exemple.com/fichier2.iso"}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) submit();
          }}
        />
        <div className="flex justify-end gap-2 mt-4">
          <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold disabled:opacity-50"
            disabled={!text.trim()}
            onClick={submit}
          >
            Ajouter
          </button>
        </div>
      </div>
    </div>
  );
}
