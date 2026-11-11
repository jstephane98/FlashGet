import { useEffect, useRef, useState } from "react";
import * as api from "../../api";
import type { EndAction } from "../../api";
import type { Settings } from "../../types";
import { t } from "../../i18n";
import type { MsgKey } from "../../i18n";
import { Card, NumberField, SectionShell, Select, SettingRow, Switch } from "./controls";

const MB = 1048576;
const ACTIONS: EndAction[] = ["none", "quit", "sleep", "hibernate", "shutdown"];
const mbText = (bps: number) => String(Math.round((bps / MB) * 100) / 100).replace(".", ",");

export function DownloadsSection({ settings, save, error, rev }: { settings: Settings; save(p: Partial<Settings>): void; error?: string; rev: number }) {
  const [endAction, setEnd] = useState<EndAction | null>(null);
  const [localError, setLocalError] = useState<string>();
  // Dernière limite non nulle : réaffichée quand on réactive l'interrupteur.
  const [lastMb, setLastMb] = useState(settings.max_speed_bps > 0 ? settings.max_speed_bps / MB : 5);

  useEffect(() => {
    let alive = true;
    api.getEndAction().then((a) => { if (alive) setEnd(a); }).catch(() => {});
    return () => { alive = false; };
  }, []);

  // Curseur : valeur locale pendant le glissement, enregistrée seulement à la fin du geste.
  const [conn, setConn] = useState(settings.connections);
  const committed = useRef(settings.connections);
  useEffect(() => { setConn(settings.connections); committed.current = settings.connections; }, [settings.connections, rev]);
  const commitConn = () => {
    if (conn === committed.current) return;
    committed.current = conn;
    save({ connections: conn });
  };

  const limitOn = settings.max_speed_bps > 0;
  const toggleLimit = (on: boolean) => {
    // On mémorise la limite courante avant de la mettre à 0, pour la rétablir à la réactivation.
    if (!on && settings.max_speed_bps > 0) setLastMb(settings.max_speed_bps / MB);
    save({ max_speed_bps: on ? Math.round(lastMb * MB) : 0 });
  };

  return (
    <SectionShell title={t("set.nav.downloads")} desc={t("set.downloads.desc")} error={error ?? localError}>
      <Card title={t("set.speedCard")}>
        <SettingRow label={t("set.concurrent")} hint={t("set.concurrentHint")}>
          <NumberField resetKey={rev} label={t("set.concurrent")} value={settings.max_concurrent} min={1} max={10}
            onCommit={(v) => save({ max_concurrent: v })} />
        </SettingRow>
        <SettingRow label={t("set.connections")} hint={t("set.connectionsHint")}>
          <input type="range" min={1} max={32} step={1} aria-label={t("set.connections")} value={conn}
            onChange={(e) => setConn(Number(e.target.value))}
            onPointerUp={commitConn} onKeyUp={commitConn} onBlur={commitConn}
            className="w-[180px] accent-accent" />
          <span className="font-mono text-[13px] text-ink w-8 text-center">{conn}</span>
        </SettingRow>
        <SettingRow label={t("set.limit")} hint={t("set.limitHint")}>
          <NumberField resetKey={rev} label={t("set.unitMbps")} decimal min={0.1} width="w-24" disabled={!limitOn}
            value={limitOn ? settings.max_speed_bps / MB : lastMb}
            format={mbText}
            onCommit={(v) => { setLastMb(v); save({ max_speed_bps: Math.round(v * MB) }); }} />
          <span className="text-[12px] text-ink-mute">{t("set.unitMbps")}</span>
          <Switch checked={limitOn} label={t("set.limit")} onChange={toggleLimit} />
        </SettingRow>
      </Card>
      <Card title={t("set.reliability")}>
        <SettingRow label={t("set.retries")} hint={t("set.retriesHint")}>
          <NumberField resetKey={rev} label={t("set.retries")} value={settings.retries} min={0} max={10}
            onCommit={(v) => save({ retries: v })} />
        </SettingRow>
        <SettingRow label={t("set.autoResume")} hint={t("set.autoResumeHint")}>
          <Switch checked={settings.auto_resume} label={t("set.autoResume")} onChange={(v) => save({ auto_resume: v })} />
        </SettingRow>
        <SettingRow label={t("set.conflict")} hint={settings.conflict === "overwrite" ? t("set.overwriteNote") : undefined}>
          <Select label={t("set.conflict")} value={settings.conflict}
            options={[
              { value: "rename", label: t("set.conflictRename") },
              { value: "overwrite", label: t("set.conflictOverwrite") },
            ]}
            onChange={(v) => save({ conflict: v })} />
        </SettingRow>
      </Card>
      <Card title={t("set.endCard")}>
        <SettingRow label={t("set.endWhen")}>
          <Select label={t("set.endWhen")} value={endAction ?? ""} disabled={endAction === null}
            options={ACTIONS.map((a) => ({ value: a, label: t(`end.${a}` as MsgKey) }))}
            onChange={(a) => {
              const prev = endAction;
              setEnd(a);
              setLocalError(undefined);
              api.setEndAction(a).catch((e) => { setEnd(prev); setLocalError(String(e)); });
            }} />
        </SettingRow>
      </Card>
    </SectionShell>
  );
}
