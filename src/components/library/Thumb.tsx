import { useEffect, useState } from "react";
import type { Download } from "../../types";
import { formatElapsed } from "../../format";
import { CategoryIcon } from "../categoryIcons";

/** Vignette partagée : image si disponible, sinon tuile de catégorie ; durée en étiquette. */
export function Thumb({ d, className, iconSize = 18 }: { d: Download; className: string; iconSize?: number }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [d.thumbnail_url]);
  const show = !!d.thumbnail_url && !failed;
  return (
    <div className={`relative rounded-md bg-surface flex items-center justify-center text-ink-mute overflow-hidden ${className}`}>
      {show ? (
        <img src={d.thumbnail_url ?? undefined} alt="" loading="lazy" onError={() => setFailed(true)}
          className="w-full h-full object-cover" />
      ) : (
        <CategoryIcon cat={d.category} size={iconSize} />
      )}
      {d.duration_s != null && (
        <span className="absolute bottom-1 right-1 px-1 text-[9px] font-mono rounded bg-black/70 text-white">{formatElapsed(d.duration_s)}</span>
      )}
    </div>
  );
}
