import { t } from "../i18n";

/** Circular health score. `null` means "no score" (scan incomplete). */
export function ScoreRing({ value }: { value: number | null }) {
  const r = 52;
  const c = 2 * Math.PI * r;
  const offset = value === null ? c : c * (1 - value / 100);
  const color =
    value === null ? "var(--unknown)" : value >= 80 ? "var(--good)" : value >= 50 ? "var(--warning)" : "var(--critical)";
  const shown = value === null ? "—" : String(value);
  return (
    <svg
      className="ring"
      width="140"
      height="140"
      viewBox="0 0 140 140"
      role="img"
      aria-label={`${t("ui.results.score")}: ${shown}`}
    >
      <circle cx="70" cy="70" r={r} fill="none" stroke="var(--border)" strokeWidth="12" />
      <circle
        className="value"
        cx="70"
        cy="70"
        r={r}
        fill="none"
        stroke={color}
        strokeWidth="12"
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={offset}
        transform="rotate(-90 70 70)"
      />
      <text x="70" y="78" textAnchor="middle" fontSize="30">
        {shown}
      </text>
    </svg>
  );
}
