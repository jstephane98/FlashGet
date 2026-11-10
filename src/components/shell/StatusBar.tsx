import { t } from "../../i18n";
import { formatSpeed } from "../../format";

interface Props {
  active: number;
  queued: number;
  maxSpeedBps: number;
  bridgeListening: boolean;
  totalSpeedBps: number;
}

export function StatusBar({ active, queued, maxSpeedBps, bridgeListening, totalSpeedBps }: Props) {
  return (
    <div className="flex h-7 shrink-0 items-center justify-between bg-deep px-3 font-sans text-[12px] text-ink-soft">
      <div className="flex items-center gap-4">
        <span>{t("status.counts", { active, queued })}</span>
        <span>
          {maxSpeedBps > 0 ? t("status.limit", { speed: formatSpeed(maxSpeedBps) }) : t("status.limit.none")}
        </span>
      </div>
      <div className="flex items-center gap-4">
        <span className="flex items-center gap-1.5">
          <span className={`h-1.5 w-1.5 rounded-full ${bridgeListening ? "bg-ok" : "bg-ink-mute"}`} />
          {bridgeListening ? t("status.bridge.on") : t("status.bridge.off")}
        </span>
        <span className="font-mono text-ink">{t("status.speed", { speed: formatSpeed(totalSpeedBps) })}</span>
      </div>
    </div>
  );
}
