import type { Priority } from "../types";

const STYLE: Record<Priority, string> = {
  high: "bg-surface text-accent-bright border border-accent/50",
  normal: "bg-surface text-ink-soft border border-border",
  low: "bg-surface text-ink-mute border border-border",
};
const LABEL: Record<Priority, string> = { high: "HAUTE", normal: "NORMALE", low: "BASSE" };

export function PriorityPill({
  value,
  onChange,
}: {
  value: Priority;
  onChange: (p: Priority) => void;
}) {
  return (
    <select
      className={`text-[10px] font-bold rounded-full px-2 py-0.5 cursor-pointer ${STYLE[value]}`}
      value={value}
      onChange={(e) => onChange(e.target.value as Priority)}
      title="Priorité"
    >
      <option value="high">{LABEL.high}</option>
      <option value="normal">{LABEL.normal}</option>
      <option value="low">{LABEL.low}</option>
    </select>
  );
}
