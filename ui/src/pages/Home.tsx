import { Info, Play } from "@phosphor-icons/react";
import { useState } from "react";
import type { ScanMode } from "../api";
import { ScoreRing } from "../components/ScoreRing";
import { t } from "../i18n";

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
  onStart: (mode: ScanMode) => void;
  lastScan?: string;
  score?: number | null;
}) {
  const [welcome, setWelcome] = useState(() => !alreadyWelcomed());
  const [deep, setDeep] = useState(false);
  return (
    <main>
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

      <div className="hero">
        <div>
          <h1>{t("ui.home.title")}</h1>
          <p className="lead">{t("ui.home.subtitle")}</p>
          <div className="cta">
            <button type="button" className="primary big" onClick={() => onStart(deep ? "deep" : "quick")}>
              <Play size={18} weight="fill" aria-hidden />
              {t("ui.home.start")}
            </button>
          </div>
          <label className="toggle">
            <input type="checkbox" checked={deep} onChange={(e) => setDeep(e.target.checked)} />
            <span className="switch" aria-hidden />
            {t("ui.home.deep")}
          </label>
        </div>
        <div className="hero-panel">
          <ScoreRing value={score ?? null} />
          <p className="muted">
            {lastScan ? t("ui.home.last_scan", { date: new Date(lastScan).toLocaleString("tr-TR") }) : t("ui.home.never")}
          </p>
        </div>
      </div>
    </main>
  );
}
