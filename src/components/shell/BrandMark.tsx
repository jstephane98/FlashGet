import { useId } from "react";

// Logo FlashGet : un F incliné dont la hampe finit en flèche, avec des traînées de vitesse.
// Même dessin que les icônes générées par scripts/brand/render_icons.py.
const F_PATH = "M795 466 H1566 L1545 644 H965 L943 833 H1418 L1397 1013 H922 L894 1252 H1027 L790 1524 L571 1252 H702 Z";
const BARS_FULL: [number, number, number][] = [
  [592, 466, 300], [463, 601, 420], [587, 751, 290], [270, 751, 258], [644, 901, 230], [443, 901, 158],
];
const BARS_SMALL: [number, number, number][] = [[560, 466, 330], [440, 740, 440]];

type Props = {
  /** Côté en pixels. */
  size: number;
  /** Version simplifiée (deux traînées, traits épaissis), pour 32 px et moins. */
  small?: boolean;
  className?: string;
};

/** L'icône de l'application : tuile bleu nuit et marque ambre. Décorative : le nom est donné à côté. */
export function BrandMark({ size, small = size <= 32, className }: Props) {
  // Identifiant de dégradé propre à chaque instance, sans caractères gênants dans url(#…).
  const grad = `fg-tile-${useId().replace(/[^\w-]/g, "")}`;
  const bars = small ? BARS_SMALL : BARS_FULL;
  const h = small ? 90 : 75;
  return (
    <svg width={size} height={size} viewBox="16 16 480 480" className={className} aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id={grad} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#16203A" />
          <stop offset="1" stopColor="#0A0F1E" />
        </linearGradient>
      </defs>
      <rect x="16" y="16" width="480" height="480" rx="112" fill={`url(#${grad})`} />
      <rect x="17.5" y="17.5" width="477" height="477" rx="110.5" fill="none" stroke="#fff" strokeOpacity=".07" strokeWidth="3" />
      <svg
        x={small ? 66 : 76}
        y={small ? 76 : 96}
        width={small ? 380 : 360}
        height={small ? 360 : 320}
        viewBox={small ? "400 446 1186 1098" : "250 446 1336 1098"}
      >
        <g fill="#F6AC33">
          <path d={F_PATH} />
          {bars.map(([x, y, w]) => (
            <rect key={`${x}-${y}`} x={x} y={y} width={w} height={h} rx={h / 2} />
          ))}
        </g>
      </svg>
    </svg>
  );
}
