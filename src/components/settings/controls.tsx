import { useEffect, useState, type ReactNode } from "react";

export const FIELD =
  "h-8 px-2.5 rounded-lg border border-border bg-field text-ink text-[13px] outline-none focus:border-accent focus:ring-1 focus:ring-accent/50 disabled:opacity-50";

/** Titre, description, erreur éventuelle, puis les cartes de la section. */
export function SectionShell({ title, desc, error, children }: { title: string; desc: string; error?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col max-w-[720px]">
      <h2 className="text-[17px] font-extrabold text-ink">{title}</h2>
      <p className="text-[13px] text-ink-mute mt-1 mb-4">{desc}</p>
      {error ? <p role="alert" className="text-[12.5px] text-err mb-3">{error}</p> : null}
      {children}
    </div>
  );
}

export function Card({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-[10px] border border-border bg-panel mb-3.5">
      <h3 className="px-4 py-[11px] text-[11px] font-bold uppercase tracking-[0.07em] text-ink-mute border-b border-line">{title}</h3>
      <div className="flex flex-col">{children}</div>
    </section>
  );
}

export function SettingRow({ label, hint, children }: { label: string; hint?: string; children?: ReactNode }) {
  return (
    <div className="flex items-center gap-4 px-4 py-3 border-b border-line last:border-b-0">
      <div className="flex-1 min-w-0">
        <div className="text-[13px] font-semibold text-ink">{label}</div>
        {hint ? <div className="text-[12px] text-ink-mute">{hint}</div> : null}
      </div>
      <div className="flex items-center gap-2 shrink-0">{children}</div>
    </div>
  );
}

export function Switch({ checked, label, onChange, disabled }: { checked: boolean; label: string; onChange(v: boolean): void; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative w-[34px] h-[19px] shrink-0 rounded-full transition-colors outline-none focus-visible:ring-1 focus-visible:ring-accent/50 disabled:opacity-50 ${checked ? "bg-accent" : "bg-line"}`}
    >
      <span className={`absolute top-[2px] left-[2px] w-[15px] h-[15px] rounded-full transition-transform ${checked ? "bg-accent-fg translate-x-[15px]" : "bg-ink-mute"}`} />
    </button>
  );
}

/** Champ numérique : la valeur n'est enregistrée qu'à la sortie du champ ou sur Entrée.
 *  `decimal` accepte la virgule française. `format` convertit la valeur enregistrée en texte. */
export function NumberField({
  value, label, min, max, decimal, disabled, width = "w-20", format, onCommit,
}: {
  value: number;
  label: string;
  min: number;
  max?: number;
  decimal?: boolean;
  disabled?: boolean;
  width?: string;
  format?: (v: number) => string;
  onCommit(v: number): void;
}) {
  const show = (v: number) => (format ? format(v) : String(v));
  const [text, setText] = useState(show(value));
  useEffect(() => { setText(show(value)); }, [value]); // eslint-disable-line react-hooks/exhaustive-deps

  const commit = () => {
    const n = Number(text.trim().replace(",", "."));
    const ok = text.trim() !== "" && Number.isFinite(n) && n >= min && (max === undefined || n <= max);
    if (!ok) return setText(show(value));
    const v = decimal ? n : Math.round(n);
    if (v !== value) onCommit(v);
    else setText(show(value));
  };

  return (
    <input
      type="text"
      inputMode={decimal ? "decimal" : "numeric"}
      aria-label={label}
      disabled={disabled}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => { if (e.key === "Enter") commit(); }}
      className={`${FIELD} ${width} font-mono text-center`}
    />
  );
}

export function Select<T extends string>({
  value, label, options, onChange, disabled,
}: {
  value: T | "";
  label: string;
  options: { value: T; label: string }[];
  onChange(v: T): void;
  disabled?: boolean;
}) {
  return (
    <select aria-label={label} className={`${FIELD} min-w-[160px]`} value={value} disabled={disabled}
      onChange={(e) => onChange(e.target.value as T)}>
      {value === "" ? <option value="" /> : null}
      {options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
    </select>
  );
}

export const ghostBtn =
  "h-8 px-3 rounded-lg border border-border bg-field text-ink text-[13px] font-semibold hover:bg-surface outline-none focus-visible:ring-1 focus-visible:ring-accent/50";
