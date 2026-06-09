import { Loader2 } from "lucide-react";

export function ProbingOverlay({ open, onClose }: { open: boolean; onClose: () => void }) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 bg-deep/80 flex items-center justify-center" onClick={onClose}>
      <div
        className="bg-bg border border-border rounded-xl p-5 w-[360px] text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)] flex items-center gap-3"
        onClick={(e) => e.stopPropagation()}
      >
        <Loader2 size={20} className="animate-spin text-accent-bright" aria-hidden="true" />
        <span role="status" className="text-sm text-ink-soft">Recherche des formats disponibles…</span>
      </div>
    </div>
  );
}
