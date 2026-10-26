import { t } from "../i18n";

/** Circular health score. `null` means "no score" (scan incomplete). */
export function ScoreRing({ value, size = 168 }: { value: number | null; size?: number }) {
  const r = 52;
  const c = 2 * Math.PI * r;
  const offset = value === null ? c : c * (1 - value / 100);
  const color =
    value === null ? "var(--unknown)" : value >= 80 ? "var(--good)" : value >= 50 ? "var(--warning)" : "var(--critical)";
  const shown = value === null ? "—" : String(value);
  return (
    <svg
      className="ring"
      width={size}
      height={size}
      viewBox="0 0 140 140"
      role="img"
      aria-label={`${t("ui.results.score")}: ${shown}`}
    >
      <circle className="track" cx="70" cy="70" r={r} fill="none" strokeWidth="10" />
      <circle
        className="value"
        cx="70"
        cy="70"
        r={r}
        fill="none"
        stroke={color}
        strokeWidth="10"
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={offset}
        transform="rotate(-90 70 70)"
      />
      <text x="70" y="82" textAnchor="middle" fontSize={value === null ? 34 : 40} aria-hidden="true">
        {value === null ? "?" : shown}
      </text>
    </svg>
  );
}
