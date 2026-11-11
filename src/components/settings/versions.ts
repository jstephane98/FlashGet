import { useEffect, useState } from "react";
import * as api from "../../api";

/** Versions de FlashGet et de yt-dlp (`null` si introuvable, `undefined` pendant le chargement). */
export function useVersions() {
  const [app, setApp] = useState<string | null | undefined>(undefined);
  const [ytdlp, setYtdlp] = useState<string | null | undefined>(undefined);
  useEffect(() => {
    let alive = true;
    api.appVersion().then((v) => { if (alive) setApp(v); }).catch(() => { if (alive) setApp(null); });
    api.getYtdlpVersion().then((v) => { if (alive) setYtdlp(v); }).catch(() => { if (alive) setYtdlp(null); });
    return () => { alive = false; };
  }, []);
  return { app, ytdlp };
}
