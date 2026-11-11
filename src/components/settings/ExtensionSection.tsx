import { useEffect, useRef, useState, type ReactNode } from "react";
import { Copy, FolderOpen, RefreshCw, X } from "lucide-react";
import * as api from "../../api";
import type { BridgeInfo, BridgeState, BridgeStatus } from "../../types";
import { t, type MsgKey } from "../../i18n";
import { fr } from "../../i18n/fr";
import { Card, FIELD, SectionShell, SettingRow, ghostBtn } from "./controls";
import { formatLastSeen } from "./extensionFormat";

const mono = (s: string | number) => <span className="font-mono">{s}</span>;

/** Comme `t`, mais les valeurs peuvent être des éléments (ports, adresses en mono). */
function rich(key: MsgKey, vars: Record<string, ReactNode>): ReactNode[] {
  return fr[key].split(/(\{\w+\})/).map((part, i) => {
    const m = /^\{(\w+)\}$/.exec(part);
    return m && m[1] in vars ? <span key={i}>{vars[m[1]]}</span> : part;
  });
}

const PILL: Record<BridgeState, { cls: string; label: MsgKey }> = {
  connected: { cls: "bg-ok/10 text-ok", label: "set.extPillConnected" },
  waiting: { cls: "bg-line text-ink-soft", label: "set.extPillWaiting" },
  rejected: { cls: "bg-err/10 text-err", label: "set.extPillRejected" },
  inactive: { cls: "bg-err/10 text-err", label: "set.extPillInactive" },
};

function StateText({ status }: { status: BridgeStatus }) {
  if (status.state === "waiting") return <>{rich("set.extWaiting", { port: mono(status.port) })}</>;
  if (status.state === "rejected") return <>{t("set.extRejected")}</>;
  if (status.state === "inactive") return <>{rich("set.extInactiveText", { port: mono(status.port) })}</>;
  const bits: ReactNode[] = [];
  if (status.browser) bits.push(status.browser);
  if (status.version) bits.push(<>{t("set.extExtension")} {mono(status.version)}</>);
  if (status.last_seen != null) bits.push(formatLastSeen(status.last_seen, Math.floor(Date.now() / 1000)));
  return <>{bits.map((b, i) => <span key={i}>{i > 0 ? " · " : null}{b}</span>)}</>;
}

const iconBtn = `${ghostBtn} inline-flex items-center gap-1.5`;

function Step({ n, title, hint, children }: { n: number; title: string; hint: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center gap-3 px-4 py-3 border-b border-line last:border-b-0">
      <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-accent-soft text-[12px] font-bold text-accent-bright">{n}</span>
      <div className="flex-1 min-w-0">
        <div className="text-[13px] font-semibold text-ink">{title}</div>
        <div className="text-[12px] text-ink-mute">{hint}</div>
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function ExtensionSection({ status }: { status: BridgeStatus | null }) {
  const [bridge, setBridge] = useState<BridgeInfo | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string>();
  const [sites, setSites] = useState<string[]>([]);
  const [draft, setDraft] = useState("");
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    api.getBridgeInfo().then((b) => { if (alive) setBridge(b); }).catch((e) => { if (alive) setError(String(e)); });
    api.getIgnoredSites().then((s) => { if (alive) setSites(s); }).catch((e) => { if (alive) setError(String(e)); });
    const un = api.onIgnoredSitesChanged((s) => { if (alive) setSites(s); });
    return () => { alive = false; clearTimeout(timer.current); un.then((f) => f()).catch(() => {}); };
  }, []);

  const copyText = async (text: string, flash: boolean) => {
    setError(undefined);
    try {
      await navigator.clipboard.writeText(text);
      if (flash) {
        setCopied(true);
        clearTimeout(timer.current);
        timer.current = setTimeout(() => setCopied(false), 1500);
      }
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
      setCopied(false);
    } catch (e) {
      setError(String(e));
    }
  };

  const openFolder = async () => {
    setError(undefined);
    try { await api.openExtensionFolder(); } catch (e) { setError(String(e)); }
  };

  const saveSites = async (next: string[]): Promise<boolean> => {
    setError(undefined);
    try {
      setSites(await api.setIgnoredSites(next));
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  };

  const add = async () => {
    const site = draft.trim();
    if (!site) return;
    if (await saveSites([...sites, site])) setDraft("");
  };

  const pill = status ? PILL[status.state] : null;

  return (
    <SectionShell title={t("set.nav.extension")} desc={t("set.extension.desc")} error={error}>
      <Card title={t("set.extConnCard")}>
        <SettingRow label={t("set.extState")}>
          {status && pill ? (
            <>
              <div className="text-[12px] text-ink-mute text-right max-w-[360px]"><StateText status={status} /></div>
              <span className={`rounded-full px-2 py-0.5 text-[11px] font-semibold whitespace-nowrap ${pill.cls}`}>{t(pill.label)}</span>
            </>
          ) : <span className="text-[12px] text-ink-mute">…</span>}
        </SettingRow>
        <SettingRow label={t("set.extToken")} hint={t("set.extTokenHint")}>
          <code className="max-w-[260px] truncate font-mono text-[12px] text-ink bg-field border border-border rounded-lg px-2.5 py-[7px]">{bridge?.token ?? "…"}</code>
          <button type="button" className={ghostBtn} disabled={!bridge} onClick={() => bridge && copyText(bridge.token, true)}>{copied ? t("set.copied") : t("set.copy")}</button>
          <button type="button" className={iconBtn} disabled={!bridge} onClick={regen}>
            <RefreshCw size={13} />{t("set.regen")}
          </button>
        </SettingRow>
      </Card>

      <Card title={t("set.extInstallCard")}>
        <Step n={1} title={t("set.extStep1")} hint={rich("set.extStep1Hint", { chrome: mono("chrome://extensions"), edge: mono("edge://extensions") })}>
          <button type="button" className={iconBtn} onClick={() => copyText("chrome://extensions", false)}>
            <Copy size={13} />{t("set.extCopyAddr")}
          </button>
        </Step>
        <Step n={2} title={t("set.extStep2")} hint={t("set.extStep2Hint")}>
          <button type="button" className={iconBtn} onClick={openFolder}>
            <FolderOpen size={13} />{t("set.extOpenFolder")}
          </button>
        </Step>
        <Step n={3} title={t("set.extStep3")} hint={t("set.extStep3Hint")}>
          <button type="button" className={iconBtn} disabled={!bridge} onClick={() => bridge && copyText(bridge.token, false)}>
            <Copy size={13} />{t("set.extCopyToken")}
          </button>
        </Step>
      </Card>

      <Card title={t("set.extSitesCard")}>
        <p className="px-4 py-3 text-[12px] text-ink-mute border-b border-line">{t("set.extSitesHint")}</p>
        {sites.length === 0 ? <p className="px-4 py-2.5 text-[12px] text-ink-mute border-b border-line">{t("set.extSitesEmpty")}</p> : null}
        {sites.map((s) => (
          <div key={s} className="flex items-center justify-between px-4 py-2 border-b border-line">
            <span className="font-mono text-[12.5px] text-ink">{s}</span>
            <button
              type="button"
              aria-label={t("set.extSiteRemove", { site: s })}
              className="p-1 rounded text-ink-mute hover:text-ink hover:bg-surface"
              onClick={() => saveSites(sites.filter((x) => x !== s))}
            >
              <X size={14} />
            </button>
          </div>
        ))}
        <div className="flex items-center gap-2 px-4 py-3">
          <input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") add(); }}
            placeholder={t("set.extSiteAddPh")}
            className={`${FIELD} flex-1 font-mono placeholder:font-sans placeholder:text-ink-mute`}
          />
          <button type="button" className={ghostBtn} onClick={add}>{t("set.extSiteAdd")}</button>
        </div>
      </Card>
    </SectionShell>
  );
}
