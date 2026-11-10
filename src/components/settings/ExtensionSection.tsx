import { useEffect, useRef, useState } from "react";
import * as api from "../../api";
import type { BridgeInfo } from "../../types";
import { t } from "../../i18n";
import { Card, SectionShell, SettingRow, ghostBtn } from "./controls";

export function ExtensionSection() {
  const [bridge, setBridge] = useState<BridgeInfo | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string>();
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    api.getBridgeInfo().then((b) => { if (alive) setBridge(b); }).catch((e) => { if (alive) setError(String(e)); });
    return () => { alive = false; clearTimeout(timer.current); };
  }, []);

  const copy = async () => {
    if (!bridge) return;
    try {
      await navigator.clipboard.writeText(bridge.token);
      setCopied(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      setError(String(e));
    }
  };

  const regen = async () => {
    if (!window.confirm(t("set.regenConfirm"))) return;
    setError(undefined);
    try {
      const token = await api.regenerateBridgeToken();
      setBridge((b) => (b ? { ...b, token } : b));
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <SectionShell title={t("set.nav.extension")} desc={t("set.extension.desc")} error={error}>
      <Card title={t("set.extStateCard")}>
        <SettingRow label={t("set.extServer")}>
          <span className={`inline-flex items-center gap-1.5 text-[13px] ${bridge?.listening ? "text-ok" : "text-ink-mute"}`}>
            <span className={`inline-block w-2 h-2 rounded-full ${bridge?.listening ? "bg-ok" : "bg-ink-mute"}`} />
            <span>{bridge?.listening ? t("set.extListening", { port: bridge.port }) : t("set.extInactive")}</span>
          </span>
        </SettingRow>
      </Card>
      <Card title={t("set.extTokenCard")}>
        <SettingRow label={t("set.extToken")} hint={t("set.extTokenHint")}>
          <code className="max-w-[260px] truncate font-mono text-[12px] text-ink bg-field border border-border rounded-lg px-2.5 py-[7px]">{bridge?.token ?? "…"}</code>
          <button type="button" className={ghostBtn} disabled={!bridge} onClick={copy}>{copied ? t("set.copied") : t("set.copy")}</button>
          <button type="button" className={ghostBtn} disabled={!bridge} onClick={regen}>{t("set.regen")}</button>
        </SettingRow>
      </Card>
      <Card title={t("set.extInstallCard")}>
        <p className="px-4 py-3 text-[13px] text-ink-soft">{t("set.extHelp")}</p>
      </Card>
    </SectionShell>
  );
}
