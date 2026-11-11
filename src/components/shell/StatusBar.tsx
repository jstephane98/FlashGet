import { t, type MsgKey } from "../../i18n";
import { formatSpeed } from "../../format";
import type { BridgeState } from "../../types";

interface Props {
  active: number;
  queued: number;
  maxSpeedBps: number;
  bridgeState: BridgeState;
  totalSpeedBps: number;
}

const BRIDGE: Record<BridgeState, { dot: string; label: MsgKey }> = {
  connected: { dot: "bg-ok", label: "status.bridge.connected" },
  waiting: { dot: "border border-ink-soft", label: "status.bridge.waiting" },
  inactive: { dot: "bg-err", label: "status.bridge.inactive" },
  rejected: { dot: "bg-err", label: "status.bridge.rejected" },
};

export function StatusBar({ active, queued, maxSpeedBps, bridgeState, totalSpeedBps }: Props) {
  const bridge = BRIDGE[bridgeState];
  return (
    <div className="flex h-7 shrink-0 items-center justify-between border-t border-border bg-deep px-3 font-sans text-[12px] text-ink-soft">
      <div className="flex items-center gap-4">
        <span>{t("status.counts", { active, queued })}</span>
        <span>
          {maxSpeedBps > 0 ? t("status.limit", { speed: formatSpeed(maxSpeedBps) }) : t("status.limit.none")}
        </span>
      </div>
      <div className="flex items-center gap-4">
        <span className="flex items-center gap-1.5">
          <span className={`h-1.5 w-1.5 rounded-full ${bridge.dot}`} />
          {t(bridge.label)}
        </span>
        <span className="font-mono text-ink">{t("status.speed", { speed: formatSpeed(totalSpeedBps) })}</span>
      </div>
    </div>
  );
}
