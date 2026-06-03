import type { Priority } from "../types";

const STYLE: Record<Priority, string> = {
  high: "bg-red-50 text-red-600",
  normal: "bg-indigo-50 text-indigo-600",
  low: "bg-gray-100 text-gray-500",
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
      className={`text-[10px] font-bold rounded-full px-2 py-0.5 border-0 cursor-pointer ${STYLE[value]}`}
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
