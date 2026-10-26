import { Info, Play } from "@phosphor-icons/react";
import { useState } from "react";
import type { Category, ScanMode } from "../api";
import { t } from "../i18n";

const AREAS: Category[] = ["security", "update", "storage", "performance", "system"];
const WELCOME_KEY = "saglik-denetimi.welcomed";

function alreadyWelcomed(): boolean {
  try {
    return localStorage.getItem(WELCOME_KEY) === "1";
  } catch {
    return false; // storage can be unavailable; showing the welcome again is harmless
  }
}

function markWelcomed() {
  try {
    localStorage.setItem(WELCOME_KEY, "1");
  } catch {
    /* ignore */
  }
}

export function Home({
  onStart,
  lastScan,
  score,
}: {
  onStart: (mode: ScanMode, categories?: Category[]) => void;
  lastScan?: string;
  score?: number | null;
}) {
  const [welcome, setWelcome] = useState(() => !alreadyWelcomed());
  const [deep, setDeep] = useState(false);
  const [areas, setAreas] = useState<Category[]>(AREAS);
  const toggleArea = (c: Category) => setAreas((a) => (a.includes(c) ? a.filter((x) => x !== c) : [...a, c]));
  const start = () => {
    const mode: ScanMode = deep ? "deep" : "quick";
    // Every area selected is the plain full scan.
    if (areas.length === AREAS.length) onStart(mode);
    else onStart(mode, AREAS.filter((c) => areas.includes(c)));
  };
  const shown = score === null || score === undefined ? "—" : String(score);
  return (
    <div className="stage home">
      <div className="stage-score">
        <div className={`bignum${shown.length > 2 ? " wide" : ""}`} role="img" aria-label={`${t("ui.results.score")}: ${shown}`}>
          {shown === "—" ? "?" : shown}
        </div>
        <p className="bignum-sub">
          {lastScan ? t("ui.home.last_scan", { date: new Date(lastScan).toLocaleString("tr-TR") }) : t("ui.home.never")}
        </p>
      </div>
      <section className="pane home-pane">
        {welcome && (
          <section className="notice" role="region" aria-label={t("ui.home.welcome_title")}>
            <Info size={20} weight="fill" aria-hidden />
            <div>
              <h2>{t("ui.home.welcome_title")}</h2>
              <p>{t("ui.home.welcome_body")}</p>
              <button
                type="button"
                className="small"
                onClick={() => {
                  markWelcomed();
                  setWelcome(false);
                }}
              >
                {t("ui.home.welcome_ok")}
              </button>
            </div>
          </section>
        )}
        <h1>{t("ui.home.title")}</h1>
        <p className="lead">{t("ui.home.subtitle")}</p>
        <fieldset className="areas">
          <legend>{t("ui.home.areas")}</legend>
          {AREAS.map((c) => (
            <label key={c} className={`chip${areas.includes(c) ? " on" : ""}`}>
              <input type="checkbox" checked={areas.includes(c)} onChange={() => toggleArea(c)} />
              {t(`category.${c}`)}
            </label>
          ))}
        </fieldset>
        {areas.length === 0 && <p className="muted">{t("ui.home.pick_one")}</p>}
        <div className="cta">
          <button type="button" className="primary big" disabled={areas.length === 0} onClick={start}>
            <Play size={18} weight="fill" aria-hidden />
            {t("ui.home.start")}
          </button>
        </div>
        <label className="toggle">
          <input type="checkbox" checked={deep} onChange={(e) => setDeep(e.target.checked)} />
          <span className="switch" aria-hidden />
          {t("ui.home.deep")}
        </label>
      </section>
    </div>
  );
}
