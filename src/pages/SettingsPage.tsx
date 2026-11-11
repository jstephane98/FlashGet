import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import type { Settings } from "../types";
import { t } from "../i18n";
import { SettingsNav, type SectionId } from "../components/settings/SettingsNav";
import { GeneralSection } from "../components/settings/GeneralSection";
import { DownloadsSection } from "../components/settings/DownloadsSection";
import { FoldersSection } from "../components/settings/FoldersSection";
import { VideoSection } from "../components/settings/VideoSection";
import { ExtensionSection } from "../components/settings/ExtensionSection";
import { NotificationsSection } from "../components/settings/NotificationsSection";
import { UpdatesSection, type UpdateFlags } from "../components/settings/UpdatesSection";
import { AboutSection } from "../components/settings/AboutSection";
import { ghostBtn } from "../components/settings/controls";

interface SettingsPageProps {
  onSaved(s: Settings): void;
  updates: UpdateFlags;
  onUpdatesChanged(patch: Partial<UpdateFlags>): void;
  /** Vrai si un enregistrement en direct est en cours (avertissement avant l'installation). */
  recording: boolean;
}

export function SettingsPage({ onSaved, updates, onUpdatesChanged, recording }: SettingsPageProps) {
  const [section, setSection] = useState<SectionId>("general");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [error, setError] = useState<string>();
  // Incrémenté à chaque refus : les champs réaffichent la dernière valeur enregistrée.
  const [rev, setRev] = useState(0);
  const saved = useRef<Settings | null>(null);
  const latest = useRef<Settings | null>(null);
  const seq = useRef(0);
  // Plus haute séquence enregistrée avec succès : fixe le point de retour d'un refus tardif.
  const lastOk = useRef(0);

  useEffect(() => {
    let alive = true;
    setLoadFailed(false);
    api.getSettings().then((s) => {
      if (!alive) return;
      saved.current = s;
      latest.current = s;
      setSettings(s);
    }).catch(() => { if (alive) setLoadFailed(true); });
    return () => { alive = false; };
  }, [attempt]);

  const save = (patch: Partial<Settings>) => {
    const base = latest.current;
    if (!base) return;
    const next = { ...base, ...patch };
    latest.current = next;
    setSettings(next);
    setError(undefined);
    const mine = ++seq.current;
    api.saveSettings(next).then(
      (res) => {
        // Un succès dépassé reste la dernière valeur connue côté serveur (point de retour).
        if (mine > lastOk.current) {
          saved.current = res;
          lastOk.current = mine;
        }
        if (mine !== seq.current) return;
        latest.current = res;
        setSettings(res);
        onSaved(res);
      },
      (e) => {
        if (mine !== seq.current) return;
        setError(String(e));
        latest.current = saved.current;
        setSettings(saved.current);
        setRev((n) => n + 1);
      },
    );
  };

  if (!settings) {
    return loadFailed ? (
      <div className="flex flex-col items-start gap-3 p-5">
        <p role="alert" className="text-[13px] text-err">{t("set.loadError")}</p>
        <button type="button" className={ghostBtn} onClick={() => setAttempt((n) => n + 1)}>{t("new.retry")}</button>
      </div>
    ) : null;
  }

  const props = { settings, save, error };
  return (
    <div className="flex h-full min-h-0">
      <SettingsNav
        active={section}
        onSelect={(id) => { setSection(id); setError(undefined); }}
        attention={{ updates: !!updates.app, video: !!updates.ytdlp }}
      />
      <div className="flex-1 min-w-0 overflow-y-auto px-7 py-[22px]">
        {section === "general" ? <GeneralSection {...props} />
          : section === "downloads" ? <DownloadsSection {...props} rev={rev} />
          : section === "folders" ? <FoldersSection />
          : section === "video" ? <VideoSection {...props} updates={updates} onUpdatesChanged={onUpdatesChanged} />
          : section === "extension" ? <ExtensionSection />
          : section === "notifications" ? <NotificationsSection {...props} />
          : section === "updates" ? <UpdatesSection {...props} updates={updates} onUpdatesChanged={onUpdatesChanged} recording={recording} />
          : <AboutSection />}
      </div>
    </div>
  );
}
